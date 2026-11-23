//! "Tributação a conferir": o que o sistema decidiu ou percebeu sozinho sobre a tributação de
//! um produto ou categoria e que alguém deve olhar — o perfil aprendido de uma nota de compra,
//! a nota que contradiz o perfil, a marca antiga de ST/monofásico que discorda do CST. Nada aqui
//! muda a nota emitida; é a lista que deixa a decisão automática visível e desfazível.

#[cfg(feature = "sqlite")]
use cardeal_kernel::Resultado;
use cardeal_kernel::{Id, Instante};
use cardeal_modkit::Consulta;
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
#[cfg(feature = "sqlite")]
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

#[cfg(feature = "sqlite")]
use crate::fiscal::CamposFiscais;
#[cfg(feature = "sqlite")]
use crate::repositorio_preco::PerfilDoGrupo;

/// De quem é a tributação a conferir.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlvoTributacao {
    /// Um produto.
    Produto(Id),
    /// Uma categoria da estrutura mercadológica.
    Grupo(Id),
}

/// Um item da lista.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TributacaoAConferir {
    /// O registro (`None` nos conflitos calculados na hora, que somem quando corrigidos).
    pub id: Option<Id>,
    /// Produto ou categoria.
    pub alvo: AlvoTributacao,
    /// O nome do produto ou da categoria.
    pub nome: String,
    /// O que aconteceu, em português.
    pub motivo: String,
    /// De onde veio ("nota 123 do fornecedor X", "cadastro").
    pub origem: String,
    /// Quando.
    pub criado_em: Option<Instante>,
}

/// As marcas antigas de ST/monofásico do grupo que discordam do CST do próprio grupo. Vale o
/// CST (é o que a nota emitida usa); a marca fica para alguém limpar.
#[cfg(feature = "sqlite")]
#[must_use]
pub fn conflitos_de_marca(perfis: &[PerfilDoGrupo]) -> Vec<TributacaoAConferir> {
    let sim_nao = |b: bool| if b { "sim" } else { "não" };
    let mut saida = Vec::new();
    for g in perfis {
        let t = g.fiscal.perfil_tributario(None);
        if let (Some(marca), Some(cst)) = (g.icms_st, g.fiscal.cst_icms.as_deref()) {
            let pelo_cst = cardeal_tributos::e_substituido(&t);
            if marca != pelo_cst {
                saida.push(TributacaoAConferir {
                    id: None,
                    alvo: AlvoTributacao::Grupo(g.id),
                    nome: g.nome.clone(),
                    motivo: format!(
                        "a marca \"ICMS já pago (ST)\" diz {}, mas o CST/CSOSN {cst} diz {}; vale o \
                         CST — confira e limpe a marca",
                        sim_nao(marca),
                        sim_nao(pelo_cst)
                    ),
                    origem: "cadastro da categoria".into(),
                    criado_em: None,
                });
            }
        }
        if let (Some(marca), Some(cst)) = (g.monofasico, g.fiscal.cst_pis.as_deref()) {
            let pelo_cst = cardeal_tributos::e_monofasico(&t);
            if marca != pelo_cst {
                saida.push(TributacaoAConferir {
                    id: None,
                    alvo: AlvoTributacao::Grupo(g.id),
                    nome: g.nome.clone(),
                    motivo: format!(
                        "a marca \"monofásico\" diz {}, mas o CST do PIS {cst} diz {}; vale o CST — \
                         confira e limpe a marca",
                        sim_nao(marca),
                        sim_nao(pelo_cst)
                    ),
                    origem: "cadastro da categoria".into(),
                    criado_em: None,
                });
            }
        }
    }
    saida
}

/// Registra um item na lista, guardando o perfil de antes (para desfazer) e o de depois.
///
/// # Errors
/// Erro de persistência.
#[cfg(feature = "sqlite")]
#[allow(clippy::too_many_arguments)]
pub fn registrar_a_conferir(
    conexao: &Connection,
    organizacao: Id,
    alvo: AlvoTributacao,
    motivo: &str,
    origem: &str,
    antes: &CamposFiscais,
    depois: &CamposFiscais,
    agora: Instante,
) -> Resultado<Id> {
    use crate::repositorio::{blob, persist};
    let id = Id::novo();
    let (produto, grupo) = match alvo {
        AlvoTributacao::Produto(p) => (Some(blob(p)), None),
        AlvoTributacao::Grupo(g) => (None, Some(blob(g))),
    };
    conexao
        .execute(
            "INSERT INTO estoque_tributacao_a_conferir
                (id, empresa, produto, grupo, motivo, origem, antes, depois, criado_em)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params![
                blob(id),
                blob(organizacao),
                produto,
                grupo,
                motivo,
                origem,
                antes.para_blob(),
                depois.para_blob(),
                agora.em_micros(),
            ],
        )
        .map_err(persist)?;
    Ok(id)
}

/// A lista "Tributação a conferir": os registros em aberto e os conflitos de marca.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ListarTributacaoAConferir;

impl Consulta for ListarTributacaoAConferir {
    type Saida = Vec<TributacaoAConferir>;
    const PERMISSAO: &'static str = "estoque.produto.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        use crate::repositorio::{blob, id_de, persist};
        let mut stmt = conexao
            .prepare(
                "SELECT a.id, a.produto, a.grupo, a.motivo, a.origem, a.criado_em,
                        COALESCE(p.nome, g.nome, '')
                 FROM estoque_tributacao_a_conferir a
                 LEFT JOIN estoque_produto p ON p.id = a.produto
                 LEFT JOIN estoque_grupo_produto g ON g.id = a.grupo
                 WHERE a.empresa = ?1 AND a.resolvido_em IS NULL
                 ORDER BY a.criado_em DESC",
            )
            .map_err(persist)?;
        let mut saida = stmt
            .query_map([blob(ctx.organizacao)], |r| {
                let produto = r.get::<_, Option<Vec<u8>>>(1)?.map(id_de);
                let grupo = r.get::<_, Option<Vec<u8>>>(2)?.map(id_de);
                Ok(TributacaoAConferir {
                    id: Some(id_de(r.get::<_, Vec<u8>>(0)?)),
                    alvo: match (produto, grupo) {
                        (Some(p), _) => AlvoTributacao::Produto(p),
                        (None, g) => AlvoTributacao::Grupo(g.unwrap_or(Id::NULO)),
                    },
                    motivo: r.get(3)?,
                    origem: r.get(4)?,
                    criado_em: Some(Instante::de_micros(r.get(5)?)),
                    nome: r.get(6)?,
                })
            })
            .map_err(persist)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(persist)?;
        let perfis = crate::repositorio_preco::perfis_dos_grupos(conexao, ctx.organizacao)?;
        saida.extend(conflitos_de_marca(&perfis));
        Ok(saida)
    }
}
