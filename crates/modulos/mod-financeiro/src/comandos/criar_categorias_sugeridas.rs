//! As categorias que toda pequena empresa usa — custos fixos (aluguel, energia, pró-labore),
//! variáveis (peças, impostos) e receitas (serviços, vendas) — criadas de uma vez, sem
//! repetir as que já existem (compara o nome sem caixa nem espaços nas pontas).

#[cfg(feature = "sqlite")]
use cardeal_kernel::{Erro, Resultado};
use cardeal_modkit::Comando;
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
use cardeal_modkit::Risco;
#[cfg(feature = "sqlite")]
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

#[cfg(feature = "sqlite")]
use crate::categoria::CategoriaFinanceira;
#[cfg(feature = "sqlite")]
use crate::repositorio::RepositorioFinanceiro;
use crate::titulo::EspecieTitulo;

/// A lista sugerida: nome e espécie.
pub const CATEGORIAS_SUGERIDAS: &[(&str, EspecieTitulo)] = &[
    ("Pró-labore", EspecieTitulo::Pagar),
    ("Salários e encargos", EspecieTitulo::Pagar),
    ("Aluguel", EspecieTitulo::Pagar),
    ("Energia elétrica", EspecieTitulo::Pagar),
    ("Água", EspecieTitulo::Pagar),
    ("Internet e telefone", EspecieTitulo::Pagar),
    ("Impostos e taxas", EspecieTitulo::Pagar),
    ("Contador", EspecieTitulo::Pagar),
    ("Peças e mercadorias", EspecieTitulo::Pagar),
    ("Ferramentas e equipamentos", EspecieTitulo::Pagar),
    ("Manutenção", EspecieTitulo::Pagar),
    ("Marketing", EspecieTitulo::Pagar),
    ("Sistemas e assinaturas", EspecieTitulo::Pagar),
    ("Tarifas bancárias", EspecieTitulo::Pagar),
    ("Transporte e combustível", EspecieTitulo::Pagar),
    ("Serviços (OS)", EspecieTitulo::Receber),
    ("Vendas de produtos", EspecieTitulo::Receber),
    ("Outras receitas", EspecieTitulo::Receber),
];

/// Cria as [`CATEGORIAS_SUGERIDAS`] que ainda faltam.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CriarCategoriasSugeridas;

impl Comando for CriarCategoriasSugeridas {
    /// Quantas foram criadas.
    type Saida = u32;
    const PERMISSAO: &'static str = "financeiro.categoria.criar";
    const RISCO: Risco = Risco::Baixo;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        let normalizar = |s: &str| s.trim().to_lowercase();
        let existentes: Vec<String> = RepositorioFinanceiro::novo(uow)
            .categorias_ativas(ctx.empresa)?
            .iter()
            .map(|c| normalizar(&c.nome))
            .collect();
        let mut criadas = 0;
        for (nome, especie) in CATEGORIAS_SUGERIDAS {
            if existentes.contains(&normalizar(nome)) {
                continue;
            }
            let c = CategoriaFinanceira::nova(ctx.empresa, *nome, Some(*especie))
                .map_err(|e| Erro::de_dominio(&e))?;
            RepositorioFinanceiro::novo(uow).inserir_categoria(&c)?;
            criadas += 1;
        }
        Ok(criadas)
    }
}
