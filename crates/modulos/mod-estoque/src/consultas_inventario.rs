//! As consultas do inventário: a lista e a posição de um inventário. A posição é **cega**
//! enquanto a contagem está aberta — a quantidade do sistema, a diferença e o valor do ajuste
//! só aparecem depois de encerrado (`docs/modulos/estoque.md` §11.5).

use cardeal_kernel::{Dinheiro, Id, Instante, Quantidade};
use cardeal_modkit::Consulta;
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
#[cfg(feature = "sqlite")]
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

#[cfg(feature = "sqlite")]
use cardeal_kernel::Resultado;

use crate::inventario::EstadoInventario;

/// Um inventário na lista.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResumoInventario {
    /// O inventário.
    pub id: Id,
    /// O local.
    pub local: Id,
    /// O nome do local.
    pub nome_local: String,
    /// A descrição.
    pub descricao: String,
    /// O estado.
    pub estado: EstadoInventario,
    /// Quantos produtos na lista.
    pub itens: u32,
    /// Quantos já contados.
    pub contados: u32,
    /// Quando foi criado.
    pub criado_em: Instante,
    /// Quando a contagem começou (o local travou).
    pub iniciado_em: Option<Instante>,
    /// Quando terminou (encerrado ou cancelado).
    pub encerrado_em: Option<Instante>,
}

#[cfg(feature = "sqlite")]
impl From<crate::repositorio_inventario::LinhaInventario> for ResumoInventario {
    fn from(l: crate::repositorio_inventario::LinhaInventario) -> Self {
        Self {
            id: l.id,
            local: l.local,
            nome_local: l.nome_local,
            descricao: l.descricao,
            estado: l.estado,
            itens: l.itens,
            contados: l.contados,
            criado_em: l.criado_em,
            iniciado_em: l.iniciado_em,
            encerrado_em: l.encerrado_em,
        }
    }
}

/// Os inventários da empresa (CNPJ), do mais recente para o mais antigo.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Inventarios;

impl Consulta for Inventarios {
    type Saida = Vec<ResumoInventario>;
    const PERMISSAO: &'static str = "estoque.inventario.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        Ok(
            crate::repositorio_inventario::listar(conexao, ctx.empresa, 100)?
                .into_iter()
                .map(Into::into)
                .collect(),
        )
    }
}

/// Um produto na posição do inventário.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemDaPosicao {
    /// O produto.
    pub produto: Id,
    /// O nome.
    pub nome: String,
    /// O código de barras, para bipar.
    pub codigo_barras: Option<String>,
    /// A sigla da unidade.
    pub unidade: String,
    /// O que foi contado.
    pub contada: Option<Quantidade>,
    /// O que o sistema tinha na foto — **só depois de encerrado**.
    pub sistema: Option<Quantidade>,
    /// Contado − sistema — **só depois de encerrado**.
    pub diferenca: Option<Quantidade>,
    /// O valor do ajuste, ao custo médio — **só depois de encerrado**, quando houve diferença.
    pub valor_ajuste: Option<Dinheiro>,
}

/// O inventário e seus itens.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PosicaoInventario {
    /// O cabeçalho.
    pub resumo: ResumoInventario,
    /// Os itens, por nome.
    pub itens: Vec<ItemDaPosicao>,
}

/// A posição de um inventário: o que foi contado e, depois de encerrado, as diferenças.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct PosicaoDoInventario {
    /// O inventário.
    pub inventario: Id,
}

impl Consulta for PosicaoDoInventario {
    type Saida = PosicaoInventario;
    const PERMISSAO: &'static str = "estoque.inventario.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        use crate::repositorio::{blob, id_de, persist};

        let resumo: ResumoInventario =
            crate::repositorio_inventario::listar(conexao, ctx.empresa, u32::MAX)?
                .into_iter()
                .find(|l| l.id == self.inventario)
                .map(Into::into)
                .ok_or_else(|| cardeal_kernel::Erro::nao_encontrado("inventário"))?;
        let revelar = resumo.estado == EstadoInventario::Encerrado;
        let mut stmt = conexao
            .prepare(
                "SELECT c.produto, p.nome, p.codigo_barras, COALESCE(u.sigla, ''),
                        c.quantidade_contada, c.quantidade_sistema, c.divergencia, c.valor_ajuste
                 FROM estoque_contagem_item c
                 JOIN estoque_produto p ON p.id = c.produto
                 LEFT JOIN estoque_unidade u ON u.id = p.unidade_padrao
                 WHERE c.inventario = ?1
                 ORDER BY p.nome ASC, p.id ASC",
            )
            .map_err(persist)?;
        let itens = stmt
            .query_map([blob(self.inventario)], |r| {
                let sistema = Quantidade::interna(r.get(5)?);
                let diferenca = r.get::<_, Option<i64>>(6)?.map(Quantidade::interna);
                let valor = r.get::<_, Option<i64>>(7)?.map(Dinheiro::centavos);
                Ok(ItemDaPosicao {
                    produto: id_de(r.get::<_, Vec<u8>>(0)?),
                    nome: r.get(1)?,
                    codigo_barras: r.get(2)?,
                    unidade: r.get(3)?,
                    contada: r.get::<_, Option<i64>>(4)?.map(Quantidade::interna),
                    sistema: revelar.then_some(sistema),
                    diferenca: diferenca.filter(|_| revelar),
                    valor_ajuste: valor.filter(|_| revelar),
                })
            })
            .map_err(persist)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(persist)?;
        Ok(PosicaoInventario { resumo, itens })
    }
}
