//! A impressão do PDV: a bobina, o comprovante automático e o rodapé (preferência da empresa),
//! e os dados do **comprovante de venda** — o recibo não fiscal, montado aqui para sair também
//! para o operador de caixa, que não vê os dados da empresa nem o cadastro de clientes.

use cardeal_kernel::{Dinheiro, Id, Instante, Preco, Quantidade};
#[cfg(feature = "sqlite")]
use cardeal_kernel::{Erro, Resultado};
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
use cardeal_modkit::{Comando, Consulta, Risco};
#[cfg(feature = "sqlite")]
use cardeal_storage::UnidadeDeTrabalho;
#[cfg(feature = "sqlite")]
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::consultas::EstornoDaVenda;
use crate::cupom::{EstadoCupom, PagamentoCupom};

/// A largura da bobina da impressora do balcão.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LarguraBobina {
    /// 80 mm — a padrão.
    #[default]
    Mm80,
    /// 58 mm — a mini.
    Mm58,
}

/// A impressão do PDV.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImpressaoPdv {
    /// A bobina.
    pub bobina: LarguraBobina,
    /// O comprovante sai sozinho ao finalizar a venda.
    pub imprimir_ao_finalizar: bool,
    /// O texto livre do rodapé do comprovante.
    pub rodape: Option<String>,
}

#[cfg(feature = "sqlite")]
fn impressao(c: &Connection, empresa: Id) -> Resultado<ImpressaoPdv> {
    use crate::repositorio::{blob, persist};
    Ok(c.query_row(
        "SELECT bobina, imprimir_comprovante, rodape FROM pdv_preferencias WHERE empresa = ?1",
        [blob(empresa)],
        |r| {
            Ok(ImpressaoPdv {
                bobina: if r.get::<_, String>(0)? == "58" {
                    LarguraBobina::Mm58
                } else {
                    LarguraBobina::Mm80
                },
                imprimir_ao_finalizar: r.get::<_, i64>(1)? != 0,
                rodape: r.get(2)?,
            })
        },
    )
    .optional()
    .map_err(persist)?
    .unwrap_or_default())
}

/// Grava a impressão do PDV.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefinirImpressaoPdv {
    /// A impressão.
    pub impressao: ImpressaoPdv,
}

impl Comando for DefinirImpressaoPdv {
    type Saida = ();
    const PERMISSAO: &'static str = "pdv.impressao.configurar";
    const RISCO: Risco = Risco::Baixo;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<()> {
        use crate::repositorio::{blob, persist};
        let i = self.impressao;
        let rodape = i
            .rodape
            .as_deref()
            .map(str::trim)
            .filter(|r| !r.is_empty())
            .map(|r| r.chars().take(200).collect::<String>());
        uow.conexao()
            .execute(
                "INSERT INTO pdv_preferencias (empresa, bobina, imprimir_comprovante, rodape)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT (empresa) DO UPDATE SET bobina = excluded.bobina,
                   imprimir_comprovante = excluded.imprimir_comprovante, rodape = excluded.rodape",
                params![
                    blob(ctx.empresa),
                    match i.bobina {
                        LarguraBobina::Mm80 => "80",
                        LarguraBobina::Mm58 => "58",
                    },
                    i.imprimir_ao_finalizar,
                    rodape
                ],
            )
            .map_err(persist)?;
        Ok(())
    }
}

/// A impressão do PDV (o padrão: bobina de 80 mm, sem comprovante automático).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ImpressaoDoPdv;

impl Consulta for ImpressaoDoPdv {
    type Saida = ImpressaoPdv;
    const PERMISSAO: &'static str = "pdv.venda.editar";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, c: &Connection) -> Resultado<ImpressaoPdv> {
        impressao(c, ctx.empresa)
    }
}

/// Uma linha do comprovante.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemDoComprovante {
    /// O nome do produto.
    pub descricao: String,
    /// A unidade ("UN").
    pub unidade: String,
    /// A quantidade.
    pub quantidade: Quantidade,
    /// O preço unitário.
    pub preco_unitario: Preco,
    /// O desconto.
    pub desconto: Dinheiro,
    /// O total, já com o desconto.
    pub total: Dinheiro,
}

/// Tudo o que o comprovante de uma venda imprime.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DadosDoComprovante {
    /// O nome da empresa (o fantasia, ou a razão social).
    pub nome: String,
    /// A razão social.
    pub razao_social: String,
    /// O CNPJ, formatado.
    pub cnpj: String,
    /// O endereço da configuração fiscal, se houver.
    pub endereco: Option<String>,
    /// O número da venda.
    pub numero: i64,
    /// Quando.
    pub abertura: Instante,
    /// O cliente identificado (nome).
    pub cliente: Option<String>,
    /// Os itens (sem os cancelados).
    pub itens: Vec<ItemDoComprovante>,
    /// Soma bruta.
    pub subtotal: Dinheiro,
    /// Descontos.
    pub desconto: Dinheiro,
    /// Total.
    pub total: Dinheiro,
    /// Os pagamentos.
    pub pagamentos: Vec<PagamentoCupom>,
    /// `Finalizado` ou `Estornado`.
    pub estado: EstadoCupom,
    /// O estorno, quando houve.
    pub estorno: Option<EstornoDaVenda>,
    /// A impressão do PDV (bobina e rodapé).
    pub impressao: ImpressaoPdv,
}

/// Os dados do comprovante de uma venda finalizada (ou estornada) — para imprimir ao finalizar
/// e para reimprimir das vendas do dia.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ComprovanteDaVenda {
    /// O cupom.
    pub cupom: Id,
}

/// `(empresa, numero, abertura, cliente, subtotal, desconto, total, estado)` de `pdv_cupom`.
#[cfg(feature = "sqlite")]
type LinhaDoCupom = (Vec<u8>, i64, i64, Option<Vec<u8>>, i64, i64, i64, String);

#[cfg(feature = "sqlite")]
fn estorno_do_cupom(c: &Connection, cupom: Id) -> Resultado<Option<EstornoDaVenda>> {
    use crate::repositorio::{blob, id_de, persist};
    c.query_row(
        "SELECT motivo, autorizado_por, estornado_em FROM pdv_estorno WHERE cupom = ?1",
        [blob(cupom)],
        |r| {
            Ok(EstornoDaVenda {
                motivo: r.get(0)?,
                autorizado_por: id_de(r.get(1)?),
                em: Instante::de_micros(r.get(2)?),
            })
        },
    )
    .optional()
    .map_err(persist)
}

/// Os itens não cancelados, com o nome e a unidade do produto.
#[cfg(feature = "sqlite")]
fn itens_do_comprovante(
    c: &Connection,
    empresa: Id,
    cupom: Id,
) -> Resultado<Vec<ItemDoComprovante>> {
    use crate::repositorio::{blob, item_de_linha, persist};
    let itens: Vec<_> = c
        .prepare(
            "SELECT id, produto, variacao, quantidade, preco_unitario, desconto_percentual,
                    desconto_valor, total_item, cancelado, motivo_cancelamento
             FROM pdv_item_cupom WHERE cupom = ?1 ORDER BY rowid",
        )
        .map_err(persist)?
        .query_map([blob(cupom)], item_de_linha)
        .map_err(persist)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(persist)?
        .into_iter()
        .filter(|i| !i.cancelado)
        .collect();
    let ids: Vec<Id> = itens.iter().map(|i| i.produto).collect();
    let produtos = mod_estoque::produtos_para_nota(c, empresa, &ids)?;
    Ok(itens
        .into_iter()
        .map(|i| {
            let p = produtos.get(&i.produto);
            ItemDoComprovante {
                descricao: p.map_or_else(|| "Produto".to_owned(), |p| p.nome.clone()),
                unidade: p.map_or_else(|| "UN".to_owned(), |p| p.unidade.clone()),
                quantidade: i.quantidade,
                preco_unitario: i.preco_unitario,
                desconto: i.desconto_valor,
                total: i.total_item,
            }
        })
        .collect())
}

/// `(nome, razão social, CNPJ formatado, endereço)` da empresa para o cabeçalho.
#[cfg(feature = "sqlite")]
fn cabecalho_da_empresa(
    c: &Connection,
    empresa: Id,
) -> Resultado<(String, String, String, Option<String>)> {
    let dados = mod_empresa::sql::dados(c, empresa)?;
    let endereco = mod_empresa::sql::dados_fiscais(c, empresa)
        .ok()
        .map(|f| f.endereco)
        .filter(|e| !e.logradouro.trim().is_empty())
        .map(|e| {
            format!(
                "{}, {} - {}, {}/{}",
                e.logradouro.trim(),
                e.numero.trim(),
                e.bairro.trim(),
                e.municipio.trim(),
                e.uf.trim()
            )
        });
    let nome = if dados.nome_fantasia.trim().is_empty() {
        dados.razao_social.clone()
    } else {
        dados.nome_fantasia.clone()
    };
    let cnpj = cardeal_kernel::Cnpj::novo(&dados.cnpj).map_or(dados.cnpj, |c| c.formatar());
    Ok((nome, dados.razao_social, cnpj, endereco))
}

impl Consulta for ComprovanteDaVenda {
    type Saida = DadosDoComprovante;
    const PERMISSAO: &'static str = "pdv.venda.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, c: &Connection) -> Resultado<DadosDoComprovante> {
        use crate::repositorio::{blob, estado_de, id_de, id_opt_de, pagamento_de_linha, persist};

        let cabecalho: Option<LinhaDoCupom> = c
            .query_row(
                "SELECT empresa, numero_terminal, abertura, cliente, subtotal, desconto, total, estado
                 FROM pdv_cupom WHERE id = ?1",
                [blob(self.cupom)],
                |r| {
                    Ok((
                        r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?,
                        r.get(7)?,
                    ))
                },
            )
            .optional()
            .map_err(persist)?;
        let Some((empresa, numero, abertura, cliente, subtotal, desconto, total, estado)) =
            cabecalho
        else {
            return Err(Erro::nao_encontrado("venda"));
        };
        if id_de(empresa) != ctx.empresa {
            return Err(Erro::nao_encontrado("venda"));
        }
        if estado_de(&estado) != EstadoCupom::Finalizado {
            return Err(Erro::novo(
                cardeal_kernel::CodigoErro::ESTADO_INVALIDO,
                "Só venda finalizada tem comprovante",
            ));
        }
        let estorno = estorno_do_cupom(c, self.cupom)?;
        let pagamentos = c
            .prepare(&format!(
                "SELECT {} FROM pdv_pagamento_cupom WHERE cupom = ?1 ORDER BY rowid",
                crate::repositorio::COLUNAS_PAGAMENTO
            ))
            .map_err(persist)?
            .query_map([blob(self.cupom)], pagamento_de_linha)
            .map_err(persist)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(persist)?;
        let (nome, razao_social, cnpj, endereco) = cabecalho_da_empresa(c, ctx.empresa)?;
        let cliente = match id_opt_de(cliente) {
            Some(id) => mod_clientes::pessoa_por_id(c, id)?.map(|p| p.nome),
            None => None,
        };
        Ok(DadosDoComprovante {
            nome,
            razao_social,
            cnpj,
            endereco,
            numero,
            abertura: Instante::de_micros(abertura),
            cliente,
            itens: itens_do_comprovante(c, ctx.empresa, self.cupom)?,
            subtotal: Dinheiro::centavos(subtotal),
            desconto: Dinheiro::centavos(desconto),
            total: Dinheiro::centavos(total),
            pagamentos,
            estado: if estorno.is_some() {
                EstadoCupom::Estornado
            } else {
                EstadoCupom::Finalizado
            },
            estorno,
            impressao: impressao(c, ctx.empresa)?,
        })
    }
}
