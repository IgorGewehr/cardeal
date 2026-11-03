//! **Transferência entre filiais** (ADR-0017 "Próximos passos", `docs/05-nucleo-financeiro.md`
//! §8): a mercadoria de um CNPJ do grupo para outro da mesma raiz, com a NF-e de transferência
//! (`mod_fiscal::transferencia`, CFOP 5152/6152) e a etapa **em trânsito** até a filial que
//! recebe conferir.
//!
//! - **Enviar** ([`EnviarTransferencia`], na filial que envia): numa transação só, a mercadoria
//!   sai do local ao **custo médio** (sem deixar saldo negativo), o Razão lança
//!   D Conta corrente com empresas do grupo / C Estoque e a NF-e vai para a fila. A rejeitada se
//!   remonta por [`EmitirNfeDaTransferencia`].
//! - **Receber** ([`ReceberTransferencia`], na filial que recebe, com a NF-e já **autorizada**):
//!   a mercadoria entra no local escolhido ao mesmo custo e o Razão lança D Estoque / C Conta
//!   corrente com empresas do grupo. Somada no grupo, a conta corrente zera — o que sobra é o
//!   que está a caminho.
//! - **Cancelar** ([`CancelarTransferencia`], na que envia, antes de receber e sem NF-e
//!   autorizada): a mercadoria volta ao local ao mesmo custo e o lançamento se estorna.
//!
//! O catálogo de produtos é do grupo (`Ctx::organizacao`): o produto é o mesmo nas duas.

#[cfg(feature = "sqlite")]
use cardeal_kernel::Resultado;
use cardeal_kernel::{Dinheiro, Id, Instante, Preco, Quantidade};
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
use cardeal_modkit::{Comando, Consulta, Risco};
#[cfg(feature = "sqlite")]
use cardeal_storage::UnidadeDeTrabalho;
#[cfg(feature = "sqlite")]
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

/// A origem dos movimentos de estoque, do lançamento e da NF-e da transferência.
pub const ORIGEM_TRANSFERENCIA: &str = "compras_transferencia";

/// O tamanho máximo da observação (vai às informações complementares da NF-e).
pub const OBSERVACAO_MAXIMA: usize = 300;

/// O mínimo de caracteres do motivo do cancelamento (o do estorno no Razão).
pub const MOTIVO_MINIMO_CANCELAMENTO: usize = 10;

/// Um produto e quanto vai.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemATransferir {
    /// O produto.
    pub produto: Id,
    /// A quantidade (positiva).
    pub quantidade: Quantidade,
}

/// Envia mercadoria para outra filial do grupo.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnviarTransferencia {
    /// A filial que recebe.
    pub destino: Id,
    /// O local de onde sai.
    pub local: Id,
    /// O que vai.
    pub itens: Vec<ItemATransferir>,
    /// Uma observação livre (até [`OBSERVACAO_MAXIMA`] caracteres).
    pub observacao: String,
}

/// Onde está a NF-e da transferência.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SituacaoDaNota {
    /// Ainda não foi criada.
    SemNota,
    /// Na fila (pendente, em contingência ou esperando a resposta).
    NaFila,
    /// Autorizada: a mercadoria pode circular.
    Autorizada,
    /// Rejeitada, com o porquê: corrija e emita de novo.
    Rejeitada(String),
    /// Cancelada (ou denegada, ou o número inutilizado).
    Cancelada,
}

impl SituacaoDaNota {
    /// O rótulo curto.
    #[must_use]
    pub const fn rotulo(&self) -> &'static str {
        match self {
            Self::SemNota => "sem NF-e",
            Self::NaFila => "NF-e na fila",
            Self::Autorizada => "NF-e autorizada",
            Self::Rejeitada(_) => "NF-e rejeitada",
            Self::Cancelada => "NF-e cancelada",
        }
    }
}

/// O que o envio fez.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferenciaEnviada {
    /// A transferência.
    pub transferencia: Id,
    /// O valor, ao custo.
    pub valor: Dinheiro,
    /// A NF-e.
    pub nota: SituacaoDaNota,
}

impl Comando for EnviarTransferencia {
    type Saida = TransferenciaEnviada;
    const PERMISSAO: &'static str = "compras.transferencia.enviar";
    const RISCO: Risco = Risco::Medio;
    const AUDITA: bool = true;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<TransferenciaEnviada> {
        sqlite::enviar(&self, ctx, uow)
    }
}

/// Cria de novo (rejeitada, remonta no mesmo número) a NF-e de uma transferência enviada.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct EmitirNfeDaTransferencia {
    /// A transferência.
    pub transferencia: Id,
}

impl Comando for EmitirNfeDaTransferencia {
    type Saida = SituacaoDaNota;
    const PERMISSAO: &'static str = "compras.transferencia.enviar";
    const RISCO: Risco = Risco::Medio;
    const AUDITA: bool = true;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<SituacaoDaNota> {
        sqlite::emitir_de_novo(self.transferencia, ctx, uow)
    }
}

/// Recebe, na filial de destino, a mercadoria de uma transferência com a NF-e autorizada.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ReceberTransferencia {
    /// A transferência.
    pub transferencia: Id,
    /// O local que recebe (da filial da sessão).
    pub local: Id,
}

impl Comando for ReceberTransferencia {
    type Saida = Dinheiro;
    const PERMISSAO: &'static str = "compras.transferencia.receber";
    const RISCO: Risco = Risco::Medio;
    const AUDITA: bool = true;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Dinheiro> {
        sqlite::receber(self, ctx, uow)
    }
}

/// Cancela uma transferência ainda não recebida e sem NF-e autorizada: a mercadoria volta.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelarTransferencia {
    /// A transferência.
    pub transferencia: Id,
    /// Por quê (ao menos [`MOTIVO_MINIMO_CANCELAMENTO`] caracteres).
    pub motivo: String,
}

impl Comando for CancelarTransferencia {
    type Saida = ();
    const PERMISSAO: &'static str = "compras.transferencia.enviar";
    const RISCO: Risco = Risco::Medio;
    const AUDITA: bool = true;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<()> {
        sqlite::cancelar(&self, ctx, uow)
    }
}

/// O estado de uma transferência.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EstadoTransferencia {
    /// Saiu da filial que envia: a caminho.
    Enviada,
    /// A filial de destino recebeu.
    Recebida,
    /// Cancelada antes de receber: a mercadoria voltou.
    Cancelada,
}

impl EstadoTransferencia {
    /// O código gravado.
    #[must_use]
    pub const fn codigo(self) -> &'static str {
        match self {
            Self::Enviada => "Enviada",
            Self::Recebida => "Recebida",
            Self::Cancelada => "Cancelada",
        }
    }

    /// O estado a partir do código.
    #[must_use]
    pub fn de_codigo(c: &str) -> Self {
        match c {
            "Recebida" => Self::Recebida,
            "Cancelada" => Self::Cancelada,
            _ => Self::Enviada,
        }
    }
}

/// Uma transferência, vista de uma das duas filiais.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferenciaEntreFiliais {
    /// Identidade.
    pub id: Id,
    /// A filial que envia.
    pub origem: Id,
    /// O nome dela.
    pub origem_nome: String,
    /// A filial que recebe.
    pub destino: Id,
    /// O nome dela.
    pub destino_nome: String,
    /// A empresa da sessão é a que enviou (senão, é a que recebe).
    pub enviada_por_esta: bool,
    /// O estado.
    pub estado: EstadoTransferencia,
    /// O valor, ao custo.
    pub valor: Dinheiro,
    /// Quantos itens.
    pub itens: u32,
    /// A observação.
    pub observacao: Option<String>,
    /// Quando saiu.
    pub enviada_em: Instante,
    /// Quando chegou.
    pub recebida_em: Option<Instante>,
    /// A NF-e (da filial que envia).
    pub nota: SituacaoDaNota,
    /// O número da NF-e, quando há.
    pub numero_nota: Option<u64>,
}

/// As transferências enviadas pela filial da sessão e as que vêm para ela, as mais novas
/// primeiro.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct TransferenciasEntreFiliais {
    /// Só as que ainda estão a caminho.
    pub so_a_caminho: bool,
}

impl Consulta for TransferenciasEntreFiliais {
    type Saida = Vec<TransferenciaEntreFiliais>;
    const PERMISSAO: &'static str = "compras.transferencia.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, c: &Connection) -> Resultado<Vec<TransferenciaEntreFiliais>> {
        sqlite::listar(c, ctx.empresa, self.so_a_caminho)
    }
}

/// Um item de uma transferência.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemDaTransferencia {
    /// O produto.
    pub produto: Id,
    /// O nome.
    pub nome: String,
    /// Quanto.
    pub quantidade: Quantidade,
    /// O custo unitário.
    pub custo_unitario: Preco,
    /// O valor.
    pub valor: Dinheiro,
}

/// Os itens de uma transferência (de uma das duas filiais).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ItensDaTransferencia {
    /// A transferência.
    pub transferencia: Id,
}

impl Consulta for ItensDaTransferencia {
    type Saida = Vec<ItemDaTransferencia>;
    const PERMISSAO: &'static str = "compras.transferencia.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, c: &Connection) -> Resultado<Vec<ItemDaTransferencia>> {
        let t = sqlite::carregar(c, self.transferencia)?;
        if t.origem != ctx.empresa && t.destino != ctx.empresa {
            return Err(cardeal_kernel::Erro::nao_encontrado("transferência"));
        }
        sqlite::itens(c, ctx.organizacao, self.transferencia)
    }
}

#[cfg(feature = "sqlite")]
pub(crate) mod sqlite {
    use std::collections::HashMap;

    use cardeal_kernel::Resultado;
    use cardeal_kernel::{CodigoErro, Data, Dinheiro, Erro, Id, Instante, Preco, Quantidade};
    use cardeal_ledger::{
        ConstrutorLancamento, Contas, Natureza, Origem, PapelConta, Razao, RepositorioRazao,
    };
    use cardeal_modkit::Ctx;
    use cardeal_storage::UnidadeDeTrabalho;
    use mod_fiscal::transferencia::{
        conferir_transferencia_comum, documento_da_transferencia_comum,
        emitir_nfe_de_transferencia_comum, DadosDaTransferencia, ItemTransferidoNaNota,
    };
    use mod_fiscal::{DocumentoResumo, EstadoDocumento};
    use rusqlite::{params, Connection, OptionalExtension};

    use super::{
        CancelarTransferencia, EnviarTransferencia, EstadoTransferencia, ItemDaTransferencia,
        ReceberTransferencia, SituacaoDaNota, TransferenciaEntreFiliais, TransferenciaEnviada,
        MOTIVO_MINIMO_CANCELAMENTO, OBSERVACAO_MAXIMA, ORIGEM_TRANSFERENCIA,
    };

    #[allow(clippy::needless_pass_by_value)]
    fn falha(e: rusqlite::Error) -> Erro {
        Erro::novo(CodigoErro::FALHA_INTERNA, e.to_string())
    }

    fn b(id: Id) -> Vec<u8> {
        id.em_bytes().to_vec()
    }

    fn id_de(v: &[u8]) -> Id {
        <[u8; 16]>::try_from(v).map_or(Id::NULO, Id::de_bytes)
    }

    fn regra(msg: impl Into<String>) -> Erro {
        Erro::novo(CodigoErro::REGRA_VIOLADA, msg.into())
    }

    /// A situação da NF-e pelo documento fiscal.
    pub(crate) fn situacao(doc: Option<&DocumentoResumo>) -> SituacaoDaNota {
        match doc {
            None => SituacaoDaNota::SemNota,
            Some(d) => match d.estado {
                EstadoDocumento::Autorizada => SituacaoDaNota::Autorizada,
                EstadoDocumento::Rejeitada => {
                    SituacaoDaNota::Rejeitada(d.motivo.clone().unwrap_or_default())
                }
                EstadoDocumento::Cancelada
                | EstadoDocumento::Denegada
                | EstadoDocumento::Inutilizada => SituacaoDaNota::Cancelada,
                _ => SituacaoDaNota::NaFila,
            },
        }
    }

    /// Uma transferência gravada.
    pub(crate) struct Gravada {
        pub(crate) origem: Id,
        pub(crate) destino: Id,
        pub(crate) estado: EstadoTransferencia,
        pub(crate) local_origem: Id,
        pub(crate) observacao: Option<String>,
        pub(crate) lancamento_envio: Option<Id>,
    }

    pub(crate) fn carregar(c: &Connection, id: Id) -> Resultado<Gravada> {
        c.query_row(
            "SELECT empresa, destino, estado, local_origem, observacao, lancamento_envio
             FROM compras_transferencia WHERE id = ?1",
            [b(id)],
            |r| {
                Ok(Gravada {
                    origem: id_de(&r.get::<_, Vec<u8>>(0)?),
                    destino: id_de(&r.get::<_, Vec<u8>>(1)?),
                    estado: EstadoTransferencia::de_codigo(&r.get::<_, String>(2)?),
                    local_origem: id_de(&r.get::<_, Vec<u8>>(3)?),
                    observacao: r.get(4)?,
                    lancamento_envio: r.get::<_, Option<Vec<u8>>>(5)?.map(|v| id_de(&v)),
                })
            },
        )
        .optional()
        .map_err(falha)?
        .ok_or_else(|| Erro::nao_encontrado("transferência"))
    }

    /// Os itens gravados: (produto, quantidade, custo unitário, valor).
    fn itens_gravados(c: &Connection, id: Id) -> Resultado<Vec<(Id, Quantidade, Preco, Dinheiro)>> {
        let mut st = c
            .prepare(
                "SELECT produto, quantidade, custo_unitario, valor
                 FROM compras_transferencia_item WHERE transferencia = ?1 ORDER BY ordem",
            )
            .map_err(falha)?;
        let v = st
            .query_map([b(id)], |r| {
                Ok((
                    id_de(&r.get::<_, Vec<u8>>(0)?),
                    Quantidade::interna(r.get(1)?),
                    Preco::interna(r.get(2)?),
                    Dinheiro::centavos(r.get(3)?),
                ))
            })
            .map_err(falha)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(falha);
        v
    }

    pub(super) fn itens(
        c: &Connection,
        organizacao: Id,
        id: Id,
    ) -> Resultado<Vec<ItemDaTransferencia>> {
        let gravados = itens_gravados(c, id)?;
        let ids: Vec<Id> = gravados.iter().map(|g| g.0).collect();
        let produtos = mod_estoque::produtos_para_nota(c, organizacao, &ids)?;
        Ok(gravados
            .into_iter()
            .map(
                |(produto, quantidade, custo_unitario, valor)| ItemDaTransferencia {
                    produto,
                    nome: produtos
                        .get(&produto)
                        .map_or_else(|| "(produto removido)".to_owned(), |p| p.nome.clone()),
                    quantidade,
                    custo_unitario,
                    valor,
                },
            )
            .collect())
    }

    fn dados_da_nota(c: &Connection, id: Id, t: &Gravada) -> Resultado<DadosDaTransferencia> {
        Ok(DadosDaTransferencia {
            destino: t.destino,
            observacao: t.observacao.clone(),
            itens: itens_gravados(c, id)?
                .into_iter()
                .map(
                    |(produto, quantidade, custo_unitario, valor)| ItemTransferidoNaNota {
                        produto,
                        quantidade,
                        custo_unitario,
                        valor,
                    },
                )
                .collect(),
        })
    }

    /// Os itens do pedido, um por produto, com quantidade positiva.
    fn agrupados(cmd: &EnviarTransferencia) -> Resultado<Vec<(Id, Quantidade)>> {
        if cmd.itens.is_empty() {
            return Err(regra("A transferência não tem itens"));
        }
        let mut v: Vec<(Id, Quantidade)> = Vec::with_capacity(cmd.itens.len());
        for i in &cmd.itens {
            if !i.quantidade.e_positiva() {
                return Err(Erro::novo(
                    CodigoErro::ENTRADA_INVALIDA,
                    "A quantidade de cada item tem de ser positiva",
                ));
            }
            match v.iter_mut().find(|(p, _)| *p == i.produto) {
                Some((_, q)) => *q += i.quantidade,
                None => v.push((i.produto, i.quantidade)),
            }
        }
        Ok(v)
    }

    fn nome_da_empresa(c: &Connection, empresa: Id) -> String {
        mod_empresa::sql::dados(c, empresa).map_or_else(
            |_| "(empresa)".to_owned(),
            |d| {
                if d.nome_fantasia.trim().is_empty() {
                    d.razao_social
                } else {
                    d.nome_fantasia
                }
            },
        )
    }

    fn conta_corrente(ctx: &Ctx, empresa: Id, uow: &mut UnidadeDeTrabalho) -> Resultado<Id> {
        let _ = ctx;
        RepositorioRazao::novo(uow)
            .garantir_conta_do_papel(
                empresa,
                PapelConta::ContaCorrenteGrupo,
                "1.2",
                "Conta corrente com empresas do grupo",
                Natureza::Ativo,
                "compras",
            )
            .map_err(|e| Erro::de_dominio(&e))
    }

    fn conta_de_estoque(empresa: Id, uow: &mut UnidadeDeTrabalho) -> Resultado<Id> {
        let repo = RepositorioRazao::novo(uow);
        Contas::nova(&repo, empresa)
            .papel(PapelConta::EstoqueMercadorias)
            .map_err(|e| Erro::de_dominio(&e))
    }

    /// O lançamento da transferência (realizado, na data de hoje).
    fn lancar(
        ctx: &Ctx,
        uow: &mut UnidadeDeTrabalho,
        id: Id,
        historico: String,
        debito: Id,
        credito: Id,
        valor: Dinheiro,
    ) -> Resultado<Id> {
        let hoje: Data = ctx.hoje();
        let lanc = ConstrutorLancamento::novo(ctx.empresa, hoje, historico)
            .origem(Origem::modulo("compras").tipo("transferencia").agregado(id))
            .criado_por(ctx.usuario, ctx.dispositivo)
            .agora(ctx.agora)
            .liquidacao(hoje)
            .debitar(debito, valor)
            .creditar(credito, valor)
            .construir()
            .map_err(|e| Erro::de_dominio(&e))?;
        Razao::registrar(&mut RepositorioRazao::novo(uow), lanc).map_err(|e| Erro::de_dominio(&e))
    }

    /// A filial e o que impede (o regime, a raiz do CNPJ, os dados fiscais das duas); devolve a
    /// observação normalizada.
    fn conferir_envio(cmd: &EnviarTransferencia, ctx: &Ctx, c: &Connection) -> Resultado<String> {
        if cmd.destino == ctx.empresa {
            return Err(regra("Escolha outro CNPJ do grupo para receber"));
        }
        let existe: bool = c
            .query_row(
                "SELECT EXISTS (SELECT 1 FROM nucleo_empresa WHERE id = ?1)",
                [b(cmd.destino)],
                |r| r.get(0),
            )
            .map_err(falha)?;
        if !existe {
            return Err(Erro::nao_encontrado("filial do grupo"));
        }
        let impedimentos = conferir_transferencia_comum(c, ctx.empresa, cmd.destino)?;
        if !impedimentos.is_empty() {
            return Err(regra(format!(
                "A transferência não pode sair: {}",
                impedimentos.join("; ")
            )));
        }
        let observacao = cmd
            .observacao
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if observacao.chars().count() > OBSERVACAO_MAXIMA {
            return Err(Erro::novo(
                CodigoErro::ENTRADA_INVALIDA,
                format!("A observação passa de {OBSERVACAO_MAXIMA} caracteres"),
            ));
        }
        Ok(observacao)
    }

    /// A saída de cada item ao custo médio, só o que há no local: (produto, quantidade, custo
    /// unitário, valor).
    fn tirar_do_estoque(
        cmd: &EnviarTransferencia,
        id: Id,
        ctx: &Ctx,
        uow: &mut UnidadeDeTrabalho,
    ) -> Resultado<Vec<(Id, Quantidade, Preco, Dinheiro)>> {
        let itens = agrupados(cmd)?;
        let ids: Vec<Id> = itens.iter().map(|i| i.0).collect();
        let produtos = mod_estoque::produtos_para_nota(uow.conexao(), ctx.organizacao, &ids)?;
        let nome = |p: Id| {
            produtos
                .get(&p)
                .map_or_else(|| "um produto".to_owned(), |x| x.nome.clone())
        };
        let mut saidas = Vec::with_capacity(itens.len());
        for (produto, q) in &itens {
            let disponivel = mod_estoque::RepositorioEstoque::novo(uow)
                .buscar_saldo(*produto, cmd.local)?
                .map_or(Quantidade::ZERO, |s| s.quantidade_disponivel);
            if disponivel < *q {
                return Err(regra(format!(
                    "{}: o local tem {disponivel}, a transferência pede {q}",
                    nome(*produto)
                )));
            }
            let s = mod_estoque::registrar_saida_por_transferencia_comum(
                mod_estoque::DadosSaida {
                    produto: *produto,
                    local: cmd.local,
                    quantidade: *q,
                    origem_modulo: ORIGEM_TRANSFERENCIA,
                    origem_id: Some(id),
                },
                ctx,
                uow,
            )?;
            if !s.aplicada.custo_total.e_positivo() {
                return Err(regra(format!(
                    "{}: sem custo no estoque — a transferência vai ao custo (dê entrada com o                      custo antes)",
                    nome(*produto)
                )));
            }
            saidas.push((
                *produto,
                *q,
                s.aplicada.custo_unitario,
                s.aplicada.custo_total,
            ));
        }
        Ok(saidas)
    }

    /// Grava a transferência enviada e os itens.
    #[allow(clippy::too_many_arguments)]
    fn gravar(
        c: &Connection,
        id: Id,
        cmd: &EnviarTransferencia,
        ctx: &Ctx,
        observacao: &str,
        valor: Dinheiro,
        lancamento: Id,
        saidas: &[(Id, Quantidade, Preco, Dinheiro)],
    ) -> Resultado<()> {
        c.execute(
            "INSERT INTO compras_transferencia
               (id, empresa, destino, local_origem, estado, observacao, valor_total,
                enviada_em, enviada_por, lancamento_envio)
             VALUES (?1, ?2, ?3, ?4, 'Enviada', ?5, ?6, ?7, ?8, ?9)",
            params![
                b(id),
                b(ctx.empresa),
                b(cmd.destino),
                b(cmd.local),
                (!observacao.is_empty()).then_some(observacao),
                valor.em_centavos(),
                ctx.agora.em_micros(),
                b(ctx.usuario),
                b(lancamento),
            ],
        )
        .map_err(falha)?;
        for (ordem, (produto, q, custo, v)) in saidas.iter().enumerate() {
            c.execute(
                "INSERT INTO compras_transferencia_item
                   (transferencia, ordem, produto, quantidade, custo_unitario, valor)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    b(id),
                    i64::try_from(ordem).unwrap_or(i64::MAX),
                    b(*produto),
                    q.unidades_internas(),
                    custo.unidades_internas(),
                    v.em_centavos()
                ],
            )
            .map_err(falha)?;
        }
        Ok(())
    }

    pub(super) fn enviar(
        cmd: &EnviarTransferencia,
        ctx: &Ctx,
        uow: &mut UnidadeDeTrabalho,
    ) -> Resultado<TransferenciaEnviada> {
        // 1. A filial e o que impede.
        let observacao = conferir_envio(cmd, ctx, uow.conexao())?;
        let id = Id::novo();

        // 2. Estoque: sai ao custo médio, só o que há no local.
        let saidas = tirar_do_estoque(cmd, id, ctx, uow)?;
        let valor: Dinheiro = saidas.iter().map(|s| s.3).sum();

        // 3. Razão: D Conta corrente com empresas do grupo / C Estoque.
        let cc = conta_corrente(ctx, ctx.empresa, uow)?;
        let estoque = conta_de_estoque(ctx.empresa, uow)?;
        let destino_nome = nome_da_empresa(uow.conexao(), cmd.destino);
        let lancamento = lancar(
            ctx,
            uow,
            id,
            format!("Transferência de mercadoria para {destino_nome}"),
            cc,
            estoque,
            valor,
        )?;

        // 4. Persistir.
        gravar(
            uow.conexao(),
            id,
            cmd,
            ctx,
            &observacao,
            valor,
            lancamento,
            &saidas,
        )?;

        // 5. A NF-e: na fila (ou rejeitada já, com o porquê).
        let t = carregar(uow.conexao(), id)?;
        let dados = dados_da_nota(uow.conexao(), id, &t)?;
        let doc = emitir_nfe_de_transferencia_comum(id, &dados, ctx, uow)?;
        Ok(TransferenciaEnviada {
            transferencia: id,
            valor,
            nota: situacao(Some(&doc)),
        })
    }

    pub(super) fn emitir_de_novo(
        id: Id,
        ctx: &Ctx,
        uow: &mut UnidadeDeTrabalho,
    ) -> Resultado<SituacaoDaNota> {
        let t = carregar(uow.conexao(), id)?;
        if t.origem != ctx.empresa {
            return Err(Erro::nao_encontrado("transferência"));
        }
        if t.estado != EstadoTransferencia::Enviada {
            return Err(Erro::novo(
                CodigoErro::ESTADO_INVALIDO,
                "Só a transferência a caminho tem NF-e a emitir",
            ));
        }
        let dados = dados_da_nota(uow.conexao(), id, &t)?;
        let doc = emitir_nfe_de_transferencia_comum(id, &dados, ctx, uow)?;
        Ok(situacao(Some(&doc)))
    }

    pub(super) fn receber(
        cmd: ReceberTransferencia,
        ctx: &Ctx,
        uow: &mut UnidadeDeTrabalho,
    ) -> Resultado<Dinheiro> {
        let t = carregar(uow.conexao(), cmd.transferencia)?;
        if t.destino != ctx.empresa {
            return Err(Erro::nao_encontrado("transferência para esta filial"));
        }
        match t.estado {
            EstadoTransferencia::Enviada => {}
            EstadoTransferencia::Recebida => {
                return Err(Erro::novo(
                    CodigoErro::ESTADO_INVALIDO,
                    "Esta transferência já foi recebida",
                ))
            }
            EstadoTransferencia::Cancelada => {
                return Err(Erro::novo(
                    CodigoErro::ESTADO_INVALIDO,
                    "Esta transferência foi cancelada pela filial que enviou",
                ))
            }
        }
        let doc = documento_da_transferencia_comum(uow.conexao(), t.origem, cmd.transferencia)?;
        let nota = situacao(doc.as_ref());
        if nota != SituacaoDaNota::Autorizada {
            return Err(Erro::novo(
                CodigoErro::ESTADO_INVALIDO,
                format!(
                    "A NF-e de transferência ainda não está autorizada ({}): a mercadoria só \
                     circula com ela",
                    nota.rotulo()
                ),
            ));
        }
        let itens = itens_gravados(uow.conexao(), cmd.transferencia)?;
        for (produto, q, custo, _) in &itens {
            mod_estoque::registrar_entrada_por_transferencia_comum(
                mod_estoque::DadosEntrada {
                    produto: *produto,
                    local: cmd.local,
                    quantidade: *q,
                    custo_unitario: *custo,
                    origem_modulo: ORIGEM_TRANSFERENCIA,
                    origem_id: Some(cmd.transferencia),
                },
                ctx,
                uow,
            )?;
        }
        let valor: Dinheiro = itens.iter().map(|i| i.3).sum();
        let estoque = conta_de_estoque(ctx.empresa, uow)?;
        let cc = conta_corrente(ctx, ctx.empresa, uow)?;
        let origem_nome = nome_da_empresa(uow.conexao(), t.origem);
        let numero = doc.map(|d| d.numero).unwrap_or_default();
        let lancamento = lancar(
            ctx,
            uow,
            cmd.transferencia,
            format!("Transferência de mercadoria de {origem_nome} — NF-e {numero}"),
            estoque,
            cc,
            valor,
        )?;
        uow.conexao()
            .execute(
                "UPDATE compras_transferencia
                 SET estado = 'Recebida', local_destino = ?2, recebida_em = ?3, recebida_por = ?4,
                     lancamento_recebimento = ?5
                 WHERE id = ?1",
                params![
                    b(cmd.transferencia),
                    b(cmd.local),
                    ctx.agora.em_micros(),
                    b(ctx.usuario),
                    b(lancamento)
                ],
            )
            .map_err(falha)?;
        Ok(valor)
    }

    pub(super) fn cancelar(
        cmd: &CancelarTransferencia,
        ctx: &Ctx,
        uow: &mut UnidadeDeTrabalho,
    ) -> Resultado<()> {
        let motivo = cmd.motivo.split_whitespace().collect::<Vec<_>>().join(" ");
        if motivo.chars().count() < MOTIVO_MINIMO_CANCELAMENTO {
            return Err(Erro::novo(
                CodigoErro::ENTRADA_INVALIDA,
                format!("Escreva o motivo (ao menos {MOTIVO_MINIMO_CANCELAMENTO} caracteres)"),
            )
            .no_campo("motivo"));
        }
        let t = carregar(uow.conexao(), cmd.transferencia)?;
        if t.origem != ctx.empresa {
            return Err(Erro::nao_encontrado("transferência"));
        }
        if t.estado != EstadoTransferencia::Enviada {
            return Err(Erro::novo(
                CodigoErro::ESTADO_INVALIDO,
                "Só a transferência a caminho se cancela",
            ));
        }
        let doc = documento_da_transferencia_comum(uow.conexao(), ctx.empresa, cmd.transferencia)?;
        match situacao(doc.as_ref()) {
            SituacaoDaNota::Autorizada => {
                return Err(Erro::novo(
                    CodigoErro::ESTADO_INVALIDO,
                    "A NF-e de transferência está autorizada: cancele-a em Notas fiscais \
                     antes (no prazo da SEFAZ) — ou a filial recebe e devolve",
                ))
            }
            SituacaoDaNota::NaFila => {
                return Err(Erro::novo(
                    CodigoErro::ESTADO_INVALIDO,
                    "A NF-e de transferência está na fila da SEFAZ: espere o desfecho",
                ))
            }
            SituacaoDaNota::SemNota | SituacaoDaNota::Rejeitada(_) | SituacaoDaNota::Cancelada => {}
        }
        mod_estoque::devolver_transferencia_da_origem(
            ORIGEM_TRANSFERENCIA,
            cmd.transferencia,
            ctx,
            uow,
        )?;
        if let Some(l) = t.lancamento_envio {
            Razao::estornar(
                &mut RepositorioRazao::novo(uow),
                l,
                &format!("Transferência cancelada: {motivo}"),
                ctx.hoje(),
            )
            .map_err(|e| Erro::de_dominio(&e))?;
        }
        let _ = t.local_origem;
        uow.conexao()
            .execute(
                "UPDATE compras_transferencia
                 SET estado = 'Cancelada', cancelada_em = ?2, motivo_cancelamento = ?3
                 WHERE id = ?1",
                params![b(cmd.transferencia), ctx.agora.em_micros(), motivo],
            )
            .map_err(falha)?;
        Ok(())
    }

    pub(super) fn listar(
        c: &Connection,
        empresa: Id,
        so_a_caminho: bool,
    ) -> Resultado<Vec<TransferenciaEntreFiliais>> {
        let mut st = c
            .prepare(
                "SELECT t.id, t.empresa, t.destino, t.estado, t.valor_total, t.observacao,
                        t.enviada_em, t.recebida_em,
                        (SELECT COUNT(*) FROM compras_transferencia_item i
                         WHERE i.transferencia = t.id)
                 FROM compras_transferencia t
                 WHERE (t.empresa = ?1 OR t.destino = ?1) AND (?2 = 0 OR t.estado = 'Enviada')
                 ORDER BY t.enviada_em DESC
                 LIMIT 500",
            )
            .map_err(falha)?;
        let linhas = st
            .query_map(params![b(empresa), i64::from(so_a_caminho)], |r| {
                Ok((
                    id_de(&r.get::<_, Vec<u8>>(0)?),
                    id_de(&r.get::<_, Vec<u8>>(1)?),
                    id_de(&r.get::<_, Vec<u8>>(2)?),
                    EstadoTransferencia::de_codigo(&r.get::<_, String>(3)?),
                    Dinheiro::centavos(r.get(4)?),
                    r.get::<_, Option<String>>(5)?,
                    Instante::de_micros(r.get(6)?),
                    r.get::<_, Option<i64>>(7)?.map(Instante::de_micros),
                    u32::try_from(r.get::<_, i64>(8)?).unwrap_or(0),
                ))
            })
            .map_err(falha)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(falha)?;
        let mut nomes: HashMap<Id, String> = HashMap::new();
        let mut v = Vec::with_capacity(linhas.len());
        for (id, origem, destino, estado, valor, observacao, enviada_em, recebida_em, itens) in
            linhas
        {
            let doc = documento_da_transferencia_comum(c, origem, id)?;
            let mut nome = |e: Id| {
                nomes
                    .entry(e)
                    .or_insert_with(|| nome_da_empresa(c, e))
                    .clone()
            };
            v.push(TransferenciaEntreFiliais {
                id,
                origem,
                origem_nome: nome(origem),
                destino,
                destino_nome: nome(destino),
                enviada_por_esta: origem == empresa,
                estado,
                valor,
                itens,
                observacao,
                enviada_em,
                recebida_em,
                nota: situacao(doc.as_ref()),
                numero_nota: doc.map(|d| d.numero),
            });
        }
        Ok(v)
    }
}
