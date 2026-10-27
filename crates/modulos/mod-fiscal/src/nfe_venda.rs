//! A **NF-e de venda** (modelo 55) de um pedido de vendas faturado: o destinatário é o cliente do
//! pedido (documento, IE, endereço com o código IBGE do município), os itens são os do pedido com
//! a mesma tributação da NFC-e, e a venda a prazo leva as parcelas a receber como duplicatas. O
//! documento nasce sem rede e vai pela mesma fila da NFC-e (sem contingência: a da NF-e é outra
//! — SVC —, e fica para depois).
//!
//! Lição do `gestao-raiz`: lá a NF-e sai de um formulário em que se digitam o destinatário e os
//! itens (ou do pedido); aqui só do pedido faturado — o cadastro do cliente e o do produto são a
//! fonte, e o que falta neles vira a frase do que corrigir.

use cardeal_fiscal::AmbienteFiscal;
use cardeal_kernel::Id;
#[cfg(feature = "sqlite")]
use cardeal_kernel::Resultado;
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
use cardeal_modkit::{Comando, Consulta, Risco};
#[cfg(feature = "sqlite")]
use cardeal_storage::UnidadeDeTrabalho;
#[cfg(feature = "sqlite")]
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::documento::DocumentoResumo;

/// A origem dos documentos da NF-e de venda.
pub const ORIGEM_VENDAS: &str = "vendas";

/// A configuração da NF-e, para a tela.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfiguracaoNfe {
    /// A série.
    pub serie: u32,
    /// O próximo número da série, no ambiente atual.
    pub proximo_numero: u64,
    /// O ambiente atual (a numeração é por ambiente).
    pub ambiente: AmbienteFiscal,
}

/// A série e o próximo número da NF-e (independentes dos da NFC-e: o modelo é outro). O próximo
/// número existe para continuar a numeração de outro sistema.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ConfigurarNfe {
    /// A série (1 a 889).
    pub serie: u32,
    /// O próximo número no ambiente atual (acima do maior já usado).
    pub proximo_numero: u64,
}

impl Comando for ConfigurarNfe {
    type Saida = ConfiguracaoNfe;
    const PERMISSAO: &'static str = "fiscal.configurar";
    const RISCO: Risco = Risco::Alto;
    const AUDITA: bool = true;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<ConfiguracaoNfe> {
        sqlite::configurar(self, ctx, uow)
    }
}

/// A configuração atual da NF-e.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct ConfiguracaoDaNfe;

impl Consulta for ConfiguracaoDaNfe {
    type Saida = ConfiguracaoNfe;
    const PERMISSAO: &'static str = "fiscal.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, c: &Connection) -> Resultado<ConfiguracaoNfe> {
        sqlite::configuracao(ctx.empresa, c)
    }
}

/// Cria a NF-e de um pedido faturado: reserva o número e guarda a nota montada — ou, se faltar
/// algo (cliente sem documento ou endereço, produto sem NCM, venda para outra UF no regime
/// normal), o documento já nasce rejeitado com o que corrigir. Não fala com a rede. Repetir
/// devolve o mesmo documento.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct EmitirNfeDoPedido {
    /// O pedido.
    pub pedido: Id,
}

impl Comando for EmitirNfeDoPedido {
    type Saida = DocumentoResumo;
    const PERMISSAO: &'static str = "fiscal.emitir";
    const RISCO: Risco = Risco::Medio;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<DocumentoResumo> {
        sqlite::emitir_do_pedido(ctx, uow, self.pedido)
    }
}

/// A NF-e de um pedido, se houver.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DocumentoDoPedido {
    /// O pedido.
    pub pedido: Id,
}

impl Consulta for DocumentoDoPedido {
    type Saida = Option<DocumentoResumo>;
    const PERMISSAO: &'static str = "fiscal.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, c: &Connection) -> Resultado<Option<DocumentoResumo>> {
        crate::sql_documento::documento_da_origem(c, ctx.empresa, ORIGEM_VENDAS, self.pedido)
    }
}

#[cfg(feature = "sqlite")]
pub(crate) mod sqlite {
    use cardeal_fiscal::{ModeloFiscal, Nfe};
    use cardeal_kernel::{CodigoErro, Dinheiro, Erro, Id, Resultado};
    use cardeal_modkit::Ctx;
    use cardeal_storage::UnidadeDeTrabalho;
    use rusqlite::Connection;

    use super::{ConfiguracaoNfe, ConfigurarNfe, ORIGEM_VENDAS};
    use crate::documento::{DocumentoResumo, EstadoDocumento};
    use crate::montagem::{destinatario_do_cliente, montar_nfe, DadosDoPedido, ItemDaVenda};
    use crate::sql_documento as sqld;

    const MODELO: ModeloFiscal = ModeloFiscal::Nfe;

    pub(super) fn configuracao(empresa: Id, c: &Connection) -> Resultado<ConfiguracaoNfe> {
        let serie = sqld::serie_nfe(c, empresa)?;
        let ambiente = crate::sql::ambiente(c, empresa)?;
        Ok(ConfiguracaoNfe {
            serie,
            proximo_numero: sqld::proximo_numero(c, empresa, MODELO, serie, ambiente)?,
            ambiente,
        })
    }

    pub(super) fn configurar(
        cmd: ConfigurarNfe,
        ctx: &Ctx,
        uow: &mut UnidadeDeTrabalho,
    ) -> Resultado<ConfiguracaoNfe> {
        let c = uow.conexao();
        if !(1..=889).contains(&cmd.serie) {
            return Err(
                Erro::novo(CodigoErro::FORA_DA_FAIXA, "A série da NF-e vai de 1 a 889")
                    .no_campo("serie"),
            );
        }
        if cmd.proximo_numero == 0 || cmd.proximo_numero > 999_999_999 {
            return Err(Erro::novo(
                CodigoErro::FORA_DA_FAIXA,
                "O próximo número vai de 1 a 999.999.999",
            )
            .no_campo("proximo_numero"));
        }
        let ambiente = crate::sql::ambiente(c, ctx.empresa)?;
        if let Some(maior) = sqld::maior_numero(c, ctx.empresa, MODELO, cmd.serie, ambiente)? {
            if cmd.proximo_numero <= maior {
                return Err(Erro::novo(
                    CodigoErro::REGRA_VIOLADA,
                    format!(
                        "O número {maior} já foi usado na série {} da NF-e — o próximo precisa \
                         ser maior",
                        cmd.serie
                    ),
                )
                .no_campo("proximo_numero"));
            }
        }
        sqld::gravar_serie_nfe(c, ctx.empresa, cmd.serie)?;
        sqld::definir_proximo(
            c,
            ctx.empresa,
            MODELO,
            cmd.serie,
            ambiente,
            cmd.proximo_numero,
        )?;
        configuracao(ctx.empresa, c)
    }

    /// O destinatário a partir do cadastro da pessoa (o cliente do pedido, o fornecedor da
    /// devolução).
    pub(crate) fn destinatario(
        c: &Connection,
        cliente: Id,
    ) -> Resultado<Result<cardeal_fiscal::Destinatario, Vec<String>>> {
        let Some(pessoa) = mod_clientes::pessoa_por_id(c, cliente)? else {
            return Ok(Err(vec!["o cliente do pedido não existe mais".into()]));
        };
        let documentos = mod_clientes::documentos_da_pessoa(c, cliente)?;
        let enderecos = mod_clientes::enderecos_da_pessoa(c, cliente)?;
        let email = mod_clientes::contatos_da_pessoa(c, cliente)?
            .into_iter()
            .filter(|ct| ct.tipo == mod_clientes::TipoContato::Email)
            .max_by_key(|ct| ct.principal)
            .map(|ct| ct.valor.trim().to_string());
        Ok(destinatario_do_cliente(
            &pessoa.nome,
            &documentos,
            &enderecos,
            email,
        ))
    }

    /// Monta a NF-e do pedido com a série/número dados — ou o que impede.
    pub(crate) fn montar_do_pedido(
        ctx: &Ctx,
        uow: &mut UnidadeDeTrabalho,
        pedido: Id,
        serie: u32,
        numero: u64,
    ) -> Resultado<(Dinheiro, Result<Nfe, Vec<String>>)> {
        let p = mod_vendas::RepositorioVendas::novo(uow)
            .buscar_pedido(pedido)?
            .ok_or_else(|| Erro::nao_encontrado("pedido"))?;
        if !matches!(
            p.estado,
            mod_vendas::EstadoPedido::Faturado
                | mod_vendas::EstadoPedido::EmEntrega
                | mod_vendas::EstadoPedido::Concluido
        ) {
            return Err(Erro::novo(
                CodigoErro::ESTADO_INVALIDO,
                "Só pedido faturado vira NF-e",
            ));
        }
        // A prazo: o faturamento lançou o título a receber do pedido; as parcelas viram as
        // duplicatas da nota.
        let parcelas = match mod_financeiro::titulo_da_origem(
            uow.conexao(),
            ctx.empresa,
            ORIGEM_VENDAS,
            pedido,
        )? {
            Some(t) => mod_financeiro::RepositorioFinanceiro::novo(uow)
                .parcelas_do_titulo(t.id)?
                .into_iter()
                .map(|parcela| (parcela.vencimento, parcela.valor))
                .collect(),
            None => Vec::new(),
        };
        let c = uow.conexao();
        let itens: Vec<ItemDaVenda> = p
            .itens
            .iter()
            .map(|i| ItemDaVenda {
                produto: i.produto,
                quantidade: i.quantidade,
                preco_unitario: i.preco_unitario,
                valor_bruto: i.total_item + i.desconto_valor,
                desconto: i.desconto_valor,
            })
            .collect();
        let total: Dinheiro = itens.iter().map(|i| i.valor_bruto - i.desconto).sum();
        let ids: Vec<Id> = itens.iter().map(|i| i.produto).collect();
        let produtos = mod_estoque::produtos_para_nota(c, ctx.organizacao, &ids)?;
        let mut problemas = Vec::new();
        if crate::sql::certificado(c, ctx.empresa)?.is_none() {
            problemas.push(
                "o certificado digital A1 não está instalado (Configurações › Fiscal)".into(),
            );
        }
        let emitente = crate::nfce::sqlite::emitente(c, ctx.empresa)?;
        let destinatario = destinatario(c, p.cliente)?;
        let nota = match (emitente, destinatario) {
            (Ok(e), Ok(d)) => montar_nfe(
                e,
                d,
                &DadosDoPedido { itens, parcelas },
                &produtos,
                serie,
                numero,
            ),
            (Err(mut a), Err(mut b)) => {
                a.append(&mut b);
                Err(a)
            }
            (Err(a), _) | (_, Err(a)) => Err(a),
        };
        let nota = match nota {
            Ok(n) if problemas.is_empty() => Ok(n),
            Ok(_) => Err(problemas),
            Err(mut p) => {
                problemas.append(&mut p);
                Err(problemas)
            }
        };
        Ok((total, nota))
    }

    pub(super) fn emitir_do_pedido(
        ctx: &Ctx,
        uow: &mut UnidadeDeTrabalho,
        pedido: Id,
    ) -> Resultado<DocumentoResumo> {
        if let Some(d) =
            sqld::documento_da_origem(uow.conexao(), ctx.empresa, ORIGEM_VENDAS, pedido)?
        {
            return Ok(d);
        }
        let (serie, ambiente) = {
            let c = uow.conexao();
            (
                sqld::serie_nfe(c, ctx.empresa)?,
                crate::sql::ambiente(c, ctx.empresa)?,
            )
        };
        let numero = sqld::reservar_numero(uow.conexao(), ctx.empresa, MODELO, serie, ambiente)?;
        let (total, nota) = montar_do_pedido(ctx, uow, pedido, serie, numero)?;
        let (estado, blob, motivo) = match nota {
            Ok(n) => (
                EstadoDocumento::Pendente,
                Some(
                    postcard::to_stdvec(&n)
                        .map_err(|e| Erro::novo(CodigoErro::FALHA_INTERNA, e.to_string()))?,
                ),
                None,
            ),
            Err(p) => (EstadoDocumento::Rejeitada, None, Some(p.join("; "))),
        };
        let c = uow.conexao();
        sqld::inserir_documento(
            c,
            &sqld::NovoDocumento {
                id: Id::novo(),
                empresa: ctx.empresa,
                modelo: MODELO,
                serie,
                numero,
                ambiente,
                origem_modulo: ORIGEM_VENDAS,
                origem: pedido,
                estado,
                total,
                nota: blob,
                motivo,
                agora: ctx.agora,
                substitui: None,
            },
        )?;
        sqld::documento_da_origem(c, ctx.empresa, ORIGEM_VENDAS, pedido)?
            .ok_or_else(|| Erro::nao_encontrado("documento fiscal"))
    }
}
