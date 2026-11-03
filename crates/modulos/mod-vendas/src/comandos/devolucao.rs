//! A **devolução de venda** (`docs/modulos/vendas.md` §4 e §5): o cliente devolve parte ou todo
//! um pedido faturado. Solicitar → aprovar → concluir, cada passo com a sua permissão (o vendedor
//! pede, o gerente aprova e conclui). Concluir, numa transação:
//! - a mercadoria volta ao estoque ao custo de quando saiu (`mod_estoque::devolver_parte_da_saida`);
//! - o dinheiro: na venda à vista, volta ao cliente pelo caixa (D Devoluções de venda / C Caixa);
//!   na venda a prazo, o título a receber é **abatido** (`mod_financeiro::abater_titulo_da_origem_comum`)
//!   e o que já tinha sido recebido vira crédito do cliente (um título a pagar a ele);
//! - o CMV volta: D Estoque / C CMV;
//! - a comissão: se o pedido já teve a comissão apurada, a parte devolvida é estornada (D
//!   Comissões a pagar / C Despesas comerciais) e descontada no próximo pagamento do vendedor;
//!   se ainda não, a apuração já sai sem ela.
//!
//! Um pedido pode voltar em várias devoluções; a última de cada item leva o resto do valor dele.

use cardeal_kernel::{Dinheiro, Id, Quantidade};
#[cfg(feature = "sqlite")]
use cardeal_kernel::{Erro, Resultado};
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
use cardeal_modkit::{Comando, Consulta, Risco};
#[cfg(feature = "sqlite")]
use cardeal_storage::UnidadeDeTrabalho;
#[cfg(feature = "sqlite")]
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::devolucao::Devolucao;

/// O mínimo de caracteres do motivo.
pub const MOTIVO_MINIMO_DEVOLUCAO_VENDA: usize = 10;

/// Pede a devolução de parte ou de todo um pedido faturado.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolicitarDevolucao {
    /// O pedido.
    pub pedido: Id,
    /// (item do pedido, quantidade que volta).
    pub itens: Vec<(Id, Quantidade)>,
    /// Por quê.
    pub motivo: String,
}

impl Comando for SolicitarDevolucao {
    type Saida = Devolucao;
    const PERMISSAO: &'static str = "vendas.devolucao.solicitar";
    const RISCO: Risco = Risco::Medio;
    const AUDITA: bool = true;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Devolucao> {
        sqlite::solicitar(&self, ctx, uow)
    }
}

/// Aprova uma devolução solicitada.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct AprovarDevolucao {
    /// A devolução.
    pub devolucao: Id,
}

impl Comando for AprovarDevolucao {
    type Saida = Devolucao;
    const PERMISSAO: &'static str = "vendas.devolucao.aprovar";
    const RISCO: Risco = Risco::Alto;
    const AUDITA: bool = true;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Devolucao> {
        let mut d = sqlite::carregar(uow.conexao(), ctx.empresa, self.devolucao)?;
        d.aprovar().map_err(|e| Erro::de_dominio(&e))?;
        sqlite::atualizar(uow.conexao(), &d)?;
        Ok(d)
    }
}

/// Cancela uma devolução ainda não concluída.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CancelarDevolucao {
    /// A devolução.
    pub devolucao: Id,
}

impl Comando for CancelarDevolucao {
    type Saida = Devolucao;
    const PERMISSAO: &'static str = "vendas.devolucao.aprovar";
    const RISCO: Risco = Risco::Medio;
    const AUDITA: bool = true;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Devolucao> {
        let mut d = sqlite::carregar(uow.conexao(), ctx.empresa, self.devolucao)?;
        d.cancelar().map_err(|e| Erro::de_dominio(&e))?;
        sqlite::atualizar(uow.conexao(), &d)?;
        Ok(d)
    }
}

/// Conclui uma devolução aprovada: estoque, dinheiro, CMV e comissão.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ConcluirDevolucao {
    /// A devolução.
    pub devolucao: Id,
}

/// O que a conclusão fez.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DevolucaoFoiConcluida {
    /// O valor devolvido.
    pub valor_total: Dinheiro,
    /// Devolvido em dinheiro (venda à vista), pelo caixa.
    pub reembolsado: Dinheiro,
    /// Abatido da conta a receber (venda a prazo).
    pub abatido: Dinheiro,
    /// O crédito do cliente (título a pagar a ele), quando a conta já tinha sido recebida.
    pub credito: Option<Id>,
    /// A comissão estornada.
    pub comissao_estornada: Dinheiro,
}

impl Comando for ConcluirDevolucao {
    type Saida = DevolucaoFoiConcluida;
    const PERMISSAO: &'static str = "vendas.devolucao.concluir";
    const RISCO: Risco = Risco::Alto;
    const AUDITA: bool = true;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<DevolucaoFoiConcluida> {
        sqlite::concluir(self.devolucao, ctx, uow)
    }
}

/// As devoluções de um pedido, da mais nova para a mais antiga.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DevolucoesDoPedido {
    /// O pedido.
    pub pedido: Id,
}

impl Consulta for DevolucoesDoPedido {
    type Saida = Vec<Devolucao>;
    const PERMISSAO: &'static str = "vendas.devolucao.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, c: &Connection) -> Resultado<Vec<Devolucao>> {
        sqlite::do_pedido(c, ctx.empresa, self.pedido)
    }
}

/// As devoluções esperando aprovação ou conclusão (a fila do gerente).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DevolucoesPendentes;

impl Consulta for DevolucoesPendentes {
    type Saida = Vec<Devolucao>;
    const PERMISSAO: &'static str = "vendas.devolucao.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, c: &Connection) -> Resultado<Vec<Devolucao>> {
        sqlite::pendentes(c, ctx.empresa)
    }
}

/// A devolução de venda, com os itens — para o `mod-fiscal` montar a NF-e de entrada.
///
/// # Errors
/// `NAO_ENCONTRADO`; os do SQLite.
#[cfg(feature = "sqlite")]
pub fn devolucao_por_id(c: &Connection, empresa: Id, id: Id) -> Resultado<Devolucao> {
    sqlite::carregar(c, empresa, id)
}

#[cfg(feature = "sqlite")]
pub(crate) mod sqlite {
    use cardeal_kernel::{CodigoErro, Dinheiro, Erro, Id, Quantidade, Resultado};
    use cardeal_ledger::{Contas, Contraparte, PapelConta, Razao, RepositorioRazao};
    use cardeal_modkit::Ctx;
    use cardeal_storage::UnidadeDeTrabalho;
    use rusqlite::{params, Connection, OptionalExtension, Row};

    use super::{DevolucaoFoiConcluida, SolicitarDevolucao, MOTIVO_MINIMO_DEVOLUCAO_VENDA};
    use crate::comandos::{autoria_de, carregar_pedido};
    use crate::devolucao::{Devolucao, EstadoDevolucao, ItemDevolvido, JaDevolvido, TipoDevolucao};
    use crate::erros::ErroVendas;
    use crate::eventos::DevolucaoConcluida;
    use crate::receituario::{concluir_devolucao, ContasFaturamento};
    use crate::repositorio::{
        blob, data_de, dias, id_de, id_opt_de, persist, versao_de, versao_i64,
    };

    fn estado_txt(e: EstadoDevolucao) -> &'static str {
        e.rotulo()
    }

    fn estado_de(s: &str) -> EstadoDevolucao {
        match s {
            "Aprovada" => EstadoDevolucao::Aprovada,
            "Concluida" => EstadoDevolucao::Concluida,
            "Cancelada" => EstadoDevolucao::Cancelada,
            _ => EstadoDevolucao::Solicitada,
        }
    }

    const COLUNAS: &str = "id, empresa, pedido, cliente, data, motivo, tipo, valor_total, estado,
                           estorno_lancamento, versao";

    fn devolucao_de(r: &Row<'_>) -> rusqlite::Result<Devolucao> {
        Ok(Devolucao {
            id: id_de(r.get(0)?),
            empresa: id_de(r.get(1)?),
            pedido_origem: id_de(r.get(2)?),
            cliente: id_de(r.get(3)?),
            data: data_de(r.get(4)?),
            motivo: r.get(5)?,
            tipo: if r.get::<_, String>(6)? == "Total" {
                TipoDevolucao::Total
            } else {
                TipoDevolucao::Parcial
            },
            itens: Vec::new(),
            valor_total: Dinheiro::centavos(r.get(7)?),
            estado: estado_de(&r.get::<_, String>(8)?),
            estorno_lancamento: id_opt_de(r.get(9)?),
            versao: versao_de(r.get(10)?),
        })
    }

    fn com_itens(c: &Connection, mut d: Devolucao) -> Resultado<Devolucao> {
        let mut stmt = c
            .prepare(
                "SELECT item_pedido, produto, variacao, quantidade, valor_estornado
                 FROM vendas_devolucao_item WHERE devolucao = ?1 ORDER BY rowid",
            )
            .map_err(persist)?;
        d.itens = stmt
            .query_map([blob(d.id)], |r| {
                Ok(ItemDevolvido {
                    item_pedido: id_de(r.get(0)?),
                    produto: id_de(r.get(1)?),
                    variacao: id_opt_de(r.get(2)?),
                    quantidade: Quantidade::interna(r.get(3)?),
                    valor_estornado: Dinheiro::centavos(r.get(4)?),
                })
            })
            .map_err(persist)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(persist)?;
        Ok(d)
    }

    pub(crate) fn carregar(c: &Connection, empresa: Id, id: Id) -> Resultado<Devolucao> {
        let d = c
            .query_row(
                &format!("SELECT {COLUNAS} FROM vendas_devolucao WHERE empresa = ?1 AND id = ?2"),
                params![blob(empresa), blob(id)],
                devolucao_de,
            )
            .optional()
            .map_err(persist)?
            .ok_or_else(|| Erro::nao_encontrado("devolução"))?;
        com_itens(c, d)
    }

    fn lista(c: &Connection, sql: &str, p: &[&dyn rusqlite::ToSql]) -> Resultado<Vec<Devolucao>> {
        let mut stmt = c.prepare(sql).map_err(persist)?;
        let v = stmt
            .query_map(p, devolucao_de)
            .map_err(persist)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(persist)?;
        v.into_iter().map(|d| com_itens(c, d)).collect()
    }

    pub(super) fn do_pedido(c: &Connection, empresa: Id, pedido: Id) -> Resultado<Vec<Devolucao>> {
        lista(
            c,
            &format!(
                "SELECT {COLUNAS} FROM vendas_devolucao WHERE empresa = ?1 AND pedido = ?2
                 ORDER BY criada_em DESC"
            ),
            &[&blob(empresa), &blob(pedido)],
        )
    }

    pub(super) fn pendentes(c: &Connection, empresa: Id) -> Resultado<Vec<Devolucao>> {
        lista(
            c,
            &format!(
                "SELECT {COLUNAS} FROM vendas_devolucao
                 WHERE empresa = ?1 AND estado IN ('Solicitada', 'Aprovada') ORDER BY criada_em"
            ),
            &[&blob(empresa)],
        )
    }

    /// O que já voltou de cada item do pedido (devoluções não canceladas).
    pub(crate) fn ja_devolvido(c: &Connection, pedido: Id) -> Resultado<JaDevolvido> {
        let mut stmt = c
            .prepare(
                "SELECT i.item_pedido, SUM(i.quantidade), SUM(i.valor_estornado)
                 FROM vendas_devolucao_item i JOIN vendas_devolucao d ON d.id = i.devolucao
                 WHERE d.pedido = ?1 AND d.estado <> 'Cancelada' GROUP BY i.item_pedido",
            )
            .map_err(persist)?;
        let v = stmt
            .query_map([blob(pedido)], |r| {
                Ok((
                    id_de(r.get(0)?),
                    (
                        Quantidade::interna(r.get(1)?),
                        Dinheiro::centavos(r.get(2)?),
                    ),
                ))
            })
            .map_err(persist)?
            .collect::<rusqlite::Result<JaDevolvido>>()
            .map_err(persist);
        v
    }

    pub(crate) fn atualizar(c: &Connection, d: &Devolucao) -> Resultado<()> {
        c.execute(
            "UPDATE vendas_devolucao SET estado = ?2, estorno_lancamento = ?3, versao = ?4
             WHERE id = ?1",
            params![
                blob(d.id),
                estado_txt(d.estado),
                d.estorno_lancamento.map(blob),
                versao_i64(d.versao)
            ],
        )
        .map(|_| ())
        .map_err(persist)
    }

    pub(super) fn solicitar(
        cmd: &SolicitarDevolucao,
        ctx: &Ctx,
        uow: &mut UnidadeDeTrabalho,
    ) -> Resultado<Devolucao> {
        let motivo = cmd.motivo.split_whitespace().collect::<Vec<_>>().join(" ");
        if motivo.chars().count() < MOTIVO_MINIMO_DEVOLUCAO_VENDA {
            return Err(Erro::novo(
                CodigoErro::ENTRADA_INVALIDA,
                format!(
                    "Explique o motivo da devolução (ao menos {MOTIVO_MINIMO_DEVOLUCAO_VENDA} \
                     caracteres)"
                ),
            )
            .no_campo("motivo"));
        }
        let pedido = carregar_pedido(uow, cmd.pedido)?;
        let ja = ja_devolvido(uow.conexao(), pedido.id)?;
        let linhas: Vec<(Id, Quantidade)> = cmd
            .itens
            .iter()
            .copied()
            .filter(|(_, q)| q.e_positiva())
            .collect();
        let d = Devolucao::solicitar_descontando(&pedido, &linhas, &ja, motivo, ctx.hoje())
            .map_err(|e| Erro::de_dominio(&e))?;
        let c = uow.conexao();
        c.execute(
            "INSERT INTO vendas_devolucao
               (id, empresa, pedido, cliente, data, motivo, tipo, valor_total, estado,
                estorno_lancamento, versao, criada_em, criada_por)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, NULL, 1, ?10, ?11)",
            params![
                blob(d.id),
                blob(ctx.empresa),
                blob(pedido.id),
                blob(d.cliente),
                dias(d.data),
                d.motivo,
                match d.tipo {
                    TipoDevolucao::Total => "Total",
                    TipoDevolucao::Parcial => "Parcial",
                },
                d.valor_total.em_centavos(),
                estado_txt(d.estado),
                ctx.agora.em_micros(),
                blob(ctx.usuario),
            ],
        )
        .map_err(persist)?;
        for i in &d.itens {
            c.execute(
                "INSERT INTO vendas_devolucao_item
                   (devolucao, item_pedido, produto, variacao, quantidade, valor_estornado)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    blob(d.id),
                    blob(i.item_pedido),
                    blob(i.produto),
                    i.variacao.map(blob),
                    i.quantidade.unidades_internas(),
                    i.valor_estornado.em_centavos()
                ],
            )
            .map_err(persist)?;
        }
        Ok(d)
    }

    /// As contas da devolução: Devoluções de venda, Caixa, CMV e Estoque.
    fn contas(ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<ContasFaturamento> {
        let repo = RepositorioRazao::novo(uow);
        let c = Contas::nova(&repo, ctx.empresa);
        let papel = |p| c.papel(p).map_err(|e| Erro::de_dominio(&e));
        Ok(ContasFaturamento {
            destino: papel(PapelConta::Caixa)?,
            receita: Id::NULO,
            descontos: papel(PapelConta::DevolucoesVenda)?,
            cmv: papel(PapelConta::Cmv)?,
            estoque: papel(PapelConta::EstoqueMercadorias)?,
        })
    }

    pub(super) fn concluir(
        id: Id,
        ctx: &Ctx,
        uow: &mut UnidadeDeTrabalho,
    ) -> Resultado<DevolucaoFoiConcluida> {
        // 1. Carregar e validar.
        let mut d = carregar(uow.conexao(), ctx.empresa, id)?;
        if d.estado != EstadoDevolucao::Aprovada {
            return Err(Erro::de_dominio(&ErroVendas::DevolucaoEmEstadoInvalido {
                atual: d.estado.rotulo(),
                esperado: EstadoDevolucao::Aprovada.rotulo(),
            }));
        }
        let pedido = carregar_pedido(uow, d.pedido_origem)?;

        // 2. Estoque: volta ao custo de quando saiu.
        let mut cmv = Dinheiro::ZERO;
        for i in &d.itens {
            let (_, reposto) = mod_estoque::devolver_parte_da_saida(
                "vendas",
                pedido.id,
                i.produto,
                i.quantidade,
                ctx,
                uow,
            )?;
            cmv += reposto;
        }

        // 3. O dinheiro: a prazo, abate o título a receber; à vista, sai do caixa.
        let a_prazo =
            mod_financeiro::titulo_da_origem(uow.conexao(), ctx.empresa, "vendas", pedido.id)?
                .is_some_and(|t| !t.esta_cancelado());
        let abatimento = if a_prazo {
            Some(mod_financeiro::abater_titulo_da_origem_comum(
                &mod_financeiro::DadosAbatimento {
                    origem_modulo: "vendas",
                    origem_id: pedido.id,
                    valor: d.valor_total,
                    papel_contrapartida: PapelConta::DevolucoesVenda,
                    origem_do_credito: "vendas_devolucao",
                    id_do_credito: d.id,
                    contraparte: Contraparte::Cliente(d.cliente),
                    historico: "Devolução de venda".into(),
                },
                ctx,
                uow,
            )?)
        } else {
            None
        };
        let contas = contas(ctx, uow)?;
        let lancamento = match concluir_devolucao(&d, !a_prazo, cmv, contas, autoria_de(ctx))
            .map_err(|e| Erro::de_dominio(&e))?
        {
            Some(l) => {
                let mut repo = RepositorioRazao::novo(uow);
                Some(Razao::registrar(&mut repo, l).map_err(|e| Erro::de_dominio(&e))?)
            }
            None => None,
        };

        // 4. Comissão: a parte devolvida sai da comissão já apurada.
        let comissao_estornada =
            crate::repositorio_comissao::estornar_da_devolucao(&d, &pedido, ctx, uow)?;

        // 5. Persistir e publicar.
        d.concluir(lancamento).map_err(|e| Erro::de_dominio(&e))?;
        atualizar(uow.conexao(), &d)?;
        uow.publicar(DevolucaoConcluida {
            devolucao: d.id,
            pedido_origem: pedido.id,
            valor_total: d.valor_total,
        })
        .map_err(|e| Erro::de_dominio(&e))?;
        Ok(DevolucaoFoiConcluida {
            valor_total: d.valor_total,
            reembolsado: if a_prazo {
                Dinheiro::ZERO
            } else {
                d.valor_total
            },
            abatido: abatimento.map_or(Dinheiro::ZERO, |a| a.abatido),
            credito: abatimento.and_then(|a| a.credito),
            comissao_estornada,
        })
    }
}
