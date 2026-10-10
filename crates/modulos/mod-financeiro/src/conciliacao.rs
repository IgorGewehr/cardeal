//! Conciliação bancária: importar o extrato (OFX/CSV), casar cada linha com o que o sistema já
//! sabe e confirmar. Uma linha do banco vira exatamente uma de três coisas: **conciliada** (casou
//! com uma baixa — já existente ou criada na hora a partir de uma parcela em aberto), **lançada**
//! (tarifa, rendimento: vira um título já quitado) ou **ignorada** (movimento que não é da
//! empresa). Nada é casado sozinho: o sistema só sugere, quem confirma é o usuário.

use cardeal_kernel::{Data, Dinheiro, Id};
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
use cardeal_modkit::Risco;
use cardeal_modkit::{Comando, Consulta};
#[cfg(feature = "sqlite")]
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

use crate::extrato::FormatoExtrato;
use crate::sugestao::{AlvoDaConciliacao, Sugestao};

#[cfg(feature = "sqlite")]
use cardeal_kernel::{CodigoErro, Erro, Resultado};
#[cfg(feature = "sqlite")]
use rusqlite::{params, Connection, OptionalExtension};

#[cfg(feature = "sqlite")]
use crate::repositorio::{blob, data_de, id_de, persist};

/// Em que pé está uma linha do extrato.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EstadoItem {
    /// Ainda sem destino.
    Pendente,
    /// Casada com uma baixa.
    Conciliado,
    /// Fora do controle da empresa.
    Ignorado,
}

impl EstadoItem {
    #[cfg(feature = "sqlite")]
    const fn txt(self) -> &'static str {
        match self {
            Self::Pendente => "Pendente",
            Self::Conciliado => "Conciliado",
            Self::Ignorado => "Ignorado",
        }
    }

    #[cfg(feature = "sqlite")]
    fn de(s: &str) -> Self {
        match s {
            "Conciliado" => Self::Conciliado,
            "Ignorado" => Self::Ignorado,
            _ => Self::Pendente,
        }
    }
}

/// Uma linha do extrato como a tela a mostra.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemDoExtrato {
    /// A linha.
    pub id: Id,
    /// A data no banco.
    pub data: Data,
    /// O texto do banco.
    pub descricao: String,
    /// O documento, se houver.
    pub documento: Option<String>,
    /// Positivo = entrou, negativo = saiu.
    pub valor: Dinheiro,
    /// O estado.
    pub estado: EstadoItem,
    /// A baixa casada, quando conciliada.
    pub baixa: Option<Id>,
    /// As melhores sugestões (só para linhas pendentes).
    pub sugestoes: Vec<Sugestao>,
}

#[cfg(feature = "sqlite")]
struct Linha {
    id: Id,
    conta: Id,
    data: Data,
    valor: Dinheiro,
    descricao: String,
    estado: EstadoItem,
}

#[cfg(feature = "sqlite")]
fn item(conexao: &Connection, empresa: Id, id: Id) -> Resultado<Linha> {
    conexao
        .query_row(
            "SELECT id, conta, data, valor, descricao, estado
             FROM financeiro_item_extrato WHERE empresa = ?1 AND id = ?2",
            params![blob(empresa), blob(id)],
            |r| {
                Ok(Linha {
                    id: id_de(r.get::<_, Vec<u8>>(0)?),
                    conta: id_de(r.get::<_, Vec<u8>>(1)?),
                    data: data_de(r.get(2)?),
                    valor: Dinheiro::centavos(r.get(3)?),
                    descricao: r.get(4)?,
                    estado: EstadoItem::de(&r.get::<_, String>(5)?),
                })
            },
        )
        .optional()
        .map_err(persist)?
        .ok_or_else(|| Erro::nao_encontrado("linha do extrato"))
}

/// Uma baixa já feita: (id, data, valor, é recebimento, texto do título).
#[cfg(feature = "sqlite")]
type BaixaSolta = (Id, Data, Dinheiro, bool, String);

/// As baixas já feitas, nesta conta, ainda não casadas com nenhuma linha do extrato.
#[cfg(feature = "sqlite")]
fn baixas_soltas(conexao: &Connection, empresa: Id, conta: Id) -> Resultado<Vec<BaixaSolta>> {
    let mut stmt = conexao
        .prepare(
            "SELECT b.id, b.data, b.valor_recebido, t.especie, COALESCE(t.observacao, '')
             FROM financeiro_baixa b
             JOIN financeiro_parcela p ON p.id = b.parcela
             JOIN financeiro_titulo t ON t.id = p.titulo
             WHERE b.empresa = ?1 AND b.estornada_em IS NULL
               AND EXISTS (SELECT 1 FROM razao_partida rp
                           WHERE rp.lancamento = b.lancamento AND rp.conta = ?2)
               AND NOT EXISTS (SELECT 1 FROM financeiro_item_extrato i
                               WHERE i.baixa = b.id AND i.estado = 'Conciliado')",
        )
        .map_err(persist)?;
    let linhas = stmt
        .query_map(params![blob(empresa), blob(conta)], |r| {
            Ok((
                id_de(r.get::<_, Vec<u8>>(0)?),
                data_de(r.get(1)?),
                Dinheiro::centavos(r.get(2)?),
                r.get::<_, String>(3)? == "Receber",
                r.get::<_, String>(4)?,
            ))
        })
        .map_err(persist)?;
    linhas
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(persist)
}

/// As linhas importadas de uma conta, com as sugestões de casamento das pendentes.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ItensDoExtrato {
    /// A conta bancária (do plano de contas) do extrato.
    pub conta: Id,
    /// Só este estado; `None` = todos.
    pub estado: Option<EstadoItem>,
}

impl Consulta for ItensDoExtrato {
    type Saida = Vec<ItemDoExtrato>;
    const PERMISSAO: &'static str = "financeiro.conciliacao.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        use crate::sugestao::{sugerir, Candidato, LinhaDoBanco};
        use crate::titulo::EspecieTitulo;

        let mut stmt = conexao
            .prepare(
                "SELECT id, data, descricao, documento, valor, estado, baixa
                 FROM financeiro_item_extrato
                 WHERE empresa = ?1 AND conta = ?2 AND (?3 IS NULL OR estado = ?3)
                 ORDER BY data DESC, rowid DESC LIMIT 5000",
            )
            .map_err(persist)?;
        let mut itens: Vec<ItemDoExtrato> = stmt
            .query_map(
                params![
                    blob(ctx.empresa),
                    blob(self.conta),
                    self.estado.map(EstadoItem::txt)
                ],
                |r| {
                    Ok(ItemDoExtrato {
                        id: id_de(r.get::<_, Vec<u8>>(0)?),
                        data: data_de(r.get(1)?),
                        descricao: r.get(2)?,
                        documento: r.get(3)?,
                        valor: Dinheiro::centavos(r.get(4)?),
                        estado: EstadoItem::de(&r.get::<_, String>(5)?),
                        baixa: r.get::<_, Option<Vec<u8>>>(6)?.map(id_de),
                        sugestoes: Vec::new(),
                    })
                },
            )
            .map_err(persist)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(persist)?;

        if itens.iter().any(|i| i.estado == EstadoItem::Pendente) {
            let mut candidatos: Vec<(bool, Candidato)> = Vec::new();
            for especie in [EspecieTitulo::Receber, EspecieTitulo::Pagar] {
                for p in crate::consultas::titulos_em_aberto(conexao, ctx.empresa, especie)? {
                    candidatos.push((
                        especie == EspecieTitulo::Receber,
                        Candidato {
                            alvo: AlvoDaConciliacao::Parcela(p.parcela),
                            valor: p.saldo(),
                            data: p.vencimento,
                            texto: p.descricao.unwrap_or_default(),
                        },
                    ));
                }
            }
            for (baixa, data, valor, receber, texto) in
                baixas_soltas(conexao, ctx.empresa, self.conta)?
            {
                candidatos.push((
                    receber,
                    Candidato {
                        alvo: AlvoDaConciliacao::Baixa(baixa),
                        valor,
                        data,
                        texto,
                    },
                ));
            }
            for i in itens
                .iter_mut()
                .filter(|i| i.estado == EstadoItem::Pendente)
            {
                let entrada = i.valor.e_positivo();
                let do_banco = LinhaDoBanco {
                    data: i.data,
                    valor: i.valor.abs(),
                    documento: i.documento.clone(),
                };
                let mesmos: Vec<Candidato> = candidatos
                    .iter()
                    .filter(|(r, _)| *r == entrada)
                    .map(|(_, c)| c.clone())
                    .collect();
                i.sugestoes = sugerir(&do_banco, &mesmos);
            }
        }
        Ok(itens)
    }
}

/// Importa um extrato para uma conta bancária.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportarExtrato {
    /// A conta bancária do plano de contas.
    pub conta: Id,
    /// O formato do arquivo.
    pub formato: FormatoExtrato,
    /// O nome do arquivo (só para o registro).
    pub arquivo: String,
    /// O conteúdo do arquivo, como texto.
    pub conteudo: String,
}

/// O que a importação fez.
#[derive(Debug, Serialize, Deserialize)]
pub struct ExtratoImportado {
    /// O registro da importação.
    pub extrato: Id,
    /// Linhas novas.
    pub importados: u32,
    /// Linhas que já tinham sido importadas antes (ignoradas).
    pub repetidos: u32,
}

impl Comando for ImportarExtrato {
    type Saida = ExtratoImportado;
    const PERMISSAO: &'static str = "financeiro.conciliacao.importar";
    const RISCO: Risco = Risco::Baixo;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        let linhas = crate::extrato::ler(self.formato, &self.conteudo)
            .map_err(|m| Erro::novo(CodigoErro::ENTRADA_INVALIDA, m))?;
        let conn = uow.conexao();
        let ok: Option<i64> = conn
            .query_row(
                "SELECT 1 FROM razao_conta WHERE empresa = ?1 AND id = ?2
                 AND natureza = 'Ativo' AND tipo = 'Analitica' AND ativa = 1",
                params![blob(ctx.empresa), blob(self.conta)],
                |r| r.get(0),
            )
            .optional()
            .map_err(persist)?;
        if ok.is_none() {
            return Err(Erro::novo(
                CodigoErro::REGRA_VIOLADA,
                "escolha uma conta bancária ativa do plano de contas",
            ));
        }
        let extrato = Id::novo();
        conn.execute(
            "INSERT INTO financeiro_extrato
                (id, empresa, conta, arquivo, formato, importado_em, importado_por, itens)
             VALUES (?1,?2,?3,?4,?5,?6,?7,0)",
            params![
                blob(extrato),
                blob(ctx.empresa),
                blob(self.conta),
                self.arquivo,
                match self.formato {
                    FormatoExtrato::Ofx => "Ofx",
                    FormatoExtrato::Csv => "Csv",
                },
                ctx.agora.em_micros(),
                blob(ctx.usuario),
            ],
        )
        .map_err(persist)?;
        let (mut novos, mut repetidos) = (0_u32, 0_u32);
        for l in &linhas {
            let n = conn
                .execute(
                    "INSERT OR IGNORE INTO financeiro_item_extrato
                        (id, extrato, empresa, conta, fitid, data, descricao, documento, valor, estado)
                     VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,'Pendente')",
                    params![
                        blob(Id::novo()),
                        blob(extrato),
                        blob(ctx.empresa),
                        blob(self.conta),
                        l.fitid,
                        i64::from(l.data.em_dias()),
                        l.descricao,
                        l.documento,
                        l.valor.em_centavos(),
                    ],
                )
                .map_err(persist)?;
            if n == 0 {
                repetidos += 1;
            } else {
                novos += 1;
            }
        }
        conn.execute(
            "UPDATE financeiro_extrato SET itens = ?2 WHERE id = ?1",
            params![blob(extrato), novos],
        )
        .map_err(persist)?;
        Ok(ExtratoImportado {
            extrato,
            importados: novos,
            repetidos,
        })
    }
}

/// Confirma o casamento de uma linha do extrato com uma parcela em aberto (cria a baixa, na
/// data do banco) ou com uma baixa que já existe (só confirma).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ConciliarItem {
    /// A linha do extrato.
    pub item: Id,
    /// Com o que casa.
    pub alvo: AlvoDaConciliacao,
}

impl Comando for ConciliarItem {
    type Saida = ();
    const PERMISSAO: &'static str = "financeiro.conciliacao.confirmar";
    const RISCO: Risco = Risco::Medio;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        use crate::meio_pagamento::MeioPagamento;
        use crate::{baixar_pagamento_comum, baixar_recebimento_comum};

        let linha = item(uow.conexao(), ctx.empresa, self.item)?;
        if linha.estado != EstadoItem::Pendente {
            return Err(Erro::novo(
                CodigoErro::ESTADO_INVALIDO,
                "esta linha do extrato já foi conciliada ou ignorada",
            ));
        }
        let entrada = linha.valor.e_positivo();
        let valor = linha.valor.abs();
        let baixa = match self.alvo {
            AlvoDaConciliacao::Parcela(parcela) => {
                let permissao = if entrada {
                    "financeiro.receber.baixar"
                } else {
                    "financeiro.pagar.baixar"
                };
                if !ctx.concede(permissao) {
                    return Err(Erro::novo(
                        CodigoErro::SEM_PERMISSAO,
                        "sem permissão para dar baixa na parcela",
                    ));
                }
                if entrada {
                    baixar_recebimento_comum(
                        parcela,
                        valor,
                        linha.data,
                        MeioPagamento::Pix,
                        Some(linha.conta),
                        ctx,
                        uow,
                    )?
                    .baixa
                } else {
                    baixar_pagamento_comum(
                        parcela,
                        valor,
                        linha.data,
                        MeioPagamento::Pix,
                        Some(linha.conta),
                        ctx,
                        uow,
                    )?
                    .baixa
                }
            }
            AlvoDaConciliacao::Baixa(baixa) => {
                let livre = baixas_soltas(uow.conexao(), ctx.empresa, linha.conta)?
                    .into_iter()
                    .find(|(id, _, v, receber, _)| {
                        *id == baixa && *v == valor && *receber == entrada
                    });
                if livre.is_none() {
                    return Err(Erro::novo(
                        CodigoErro::REGRA_VIOLADA,
                        "essa baixa não bate com a linha do extrato (valor, sentido ou já casada)",
                    ));
                }
                baixa
            }
        };
        uow.conexao()
            .execute(
                "UPDATE financeiro_item_extrato SET estado = 'Conciliado', baixa = ?2 WHERE id = ?1",
                params![blob(linha.id), blob(baixa)],
            )
            .map_err(persist)?;
        Ok(())
    }
}

/// Marca uma linha como fora do controle da empresa (ou a traz de volta, desfazendo).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct IgnorarItemDoExtrato {
    /// A linha.
    pub item: Id,
    /// `true` ignora; `false` devolve a ignorada para pendente.
    pub ignorar: bool,
}

impl Comando for IgnorarItemDoExtrato {
    type Saida = ();
    const PERMISSAO: &'static str = "financeiro.conciliacao.confirmar";
    const RISCO: Risco = Risco::Baixo;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        let linha = item(uow.conexao(), ctx.empresa, self.item)?;
        let (de, para) = if self.ignorar {
            (EstadoItem::Pendente, EstadoItem::Ignorado)
        } else {
            (EstadoItem::Ignorado, EstadoItem::Pendente)
        };
        if linha.estado != de {
            return Err(Erro::novo(
                CodigoErro::ESTADO_INVALIDO,
                "a linha não está no estado esperado para esta ação",
            ));
        }
        uow.conexao()
            .execute(
                "UPDATE financeiro_item_extrato SET estado = ?2 WHERE id = ?1",
                params![blob(linha.id), para.txt()],
            )
            .map_err(persist)?;
        Ok(())
    }
}

/// Transforma uma linha do extrato sem correspondência (tarifa, rendimento, pagamento de algo
/// que ninguém lançou) num título **já quitado** na conta do extrato, e a concilia.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LancarItemDoExtrato {
    /// A linha.
    pub item: Id,
    /// A categoria do lançamento (e, se vinculada a uma conta, onde ele cai no plano).
    pub categoria: Option<Id>,
    /// O que escrever no título; vazio usa o texto do banco.
    pub descricao: Option<String>,
}

impl Comando for LancarItemDoExtrato {
    type Saida = ();
    const PERMISSAO: &'static str = "financeiro.conciliacao.confirmar";
    const RISCO: Risco = Risco::Medio;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        use crate::meio_pagamento::MeioPagamento;
        use crate::{LancarTituloAPagar, LancarTituloAReceber, QuitadoAgora};

        let linha = item(uow.conexao(), ctx.empresa, self.item)?;
        if linha.estado != EstadoItem::Pendente {
            return Err(Erro::novo(
                CodigoErro::ESTADO_INVALIDO,
                "esta linha do extrato já foi conciliada ou ignorada",
            ));
        }
        let entrada = linha.valor.e_positivo();
        let permissao = if entrada {
            "financeiro.receber.criar"
        } else {
            "financeiro.pagar.criar"
        };
        if !ctx.concede(permissao) {
            return Err(Erro::novo(
                CodigoErro::SEM_PERMISSAO,
                "sem permissão para lançar este movimento",
            ));
        }
        let observacao = self
            .descricao
            .filter(|d| !d.trim().is_empty())
            .unwrap_or_else(|| linha.descricao.clone());
        let quitado = Some(QuitadoAgora {
            meio_pagamento: MeioPagamento::Pix,
            conta: Some(linha.conta),
        });
        let parcela = if entrada {
            let r = LancarTituloAReceber {
                cliente: None,
                valor_total: linha.valor.abs(),
                emissao: linha.data,
                parcelas: 1,
                primeiro_vencimento: linha.data,
                intervalo_dias: 0,
                observacao: Some(observacao),
                categoria: self.categoria,
                quitado_agora: quitado,
            }
            .executar(ctx, uow)?;
            r.parcelas[0]
        } else {
            let r = LancarTituloAPagar {
                fornecedor: None,
                valor_total: linha.valor.abs(),
                emissao: linha.data,
                parcelas: 1,
                primeiro_vencimento: linha.data,
                intervalo_dias: 0,
                observacao: Some(observacao),
                categoria: self.categoria,
                quitado_agora: quitado,
            }
            .executar(ctx, uow)?;
            r.parcelas[0]
        };
        let baixa: Vec<u8> = uow
            .conexao()
            .query_row(
                "SELECT id FROM financeiro_baixa WHERE parcela = ?1 ORDER BY criado_em DESC LIMIT 1",
                [blob(parcela)],
                |r| r.get(0),
            )
            .map_err(persist)?;
        uow.conexao()
            .execute(
                "UPDATE financeiro_item_extrato SET estado = 'Conciliado', baixa = ?2 WHERE id = ?1",
                params![blob(linha.id), baixa],
            )
            .map_err(persist)?;
        Ok(())
    }
}
