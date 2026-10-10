//! Modelos de plano de contas por segmento: contas **a mais** que se juntam ao plano gerencial
//! padrão (`plano_padrao`, semeado em toda empresa), para quem quer ver aluguel, energia,
//! pró-labore ou serviços de bancada separados na DRE em vez de tudo numa linha só.
//!
//! Aplicar um modelo é idempotente (cria só o que falta, por código) e nunca mexe em conta
//! existente. As contas novas ficam sob as sintéticas `4` (Receitas) e `5` (Custos e Despesas),
//! numeradas a partir de `4.20` e `5.20` para não colidir com o plano base; as de receita levam
//! o papel da receita que representam (vendas ou serviços), para a DRE agrupá-las certo.

use serde::{Deserialize, Serialize};

use crate::conta::{Natureza, PapelConta};

/// O segmento do negócio.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ModeloDePlano {
    /// Comércio varejista ou atacadista.
    Comercio,
    /// Prestação de serviços.
    Servicos,
    /// Assistência técnica e oficina (peças + serviço de bancada).
    AssistenciaTecnica,
}

impl ModeloDePlano {
    /// Todos, na ordem em que a tela os mostra.
    pub const TODOS: [Self; 3] = [Self::Comercio, Self::Servicos, Self::AssistenciaTecnica];

    /// O nome para o usuário.
    #[must_use]
    pub const fn nome(self) -> &'static str {
        match self {
            Self::Comercio => "Comércio",
            Self::Servicos => "Prestação de serviços",
            Self::AssistenciaTecnica => "Assistência técnica / oficina",
        }
    }

    /// Uma frase sobre o que o modelo acrescenta.
    #[must_use]
    pub const fn descricao(self) -> &'static str {
        match self {
            Self::Comercio => {
                "Despesas do dia a dia separadas (aluguel, energia, pró-labore, marketing…) \
                 mais fretes, embalagens, comissões e taxas de marketplace."
            }
            Self::Servicos => {
                "As despesas do dia a dia separadas, receitas de serviços recorrentes e avulsos, \
                 subcontratação e viagens."
            }
            Self::AssistenciaTecnica => {
                "As despesas do dia a dia separadas, receitas de bancada e de peças, e custos de \
                 peças, ferramentas, garantia e laboratório terceirizado."
            }
        }
    }
}

/// Uma conta de um modelo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContaDeModelo {
    /// O código (`"5.20"`); o pai é o código sem o último segmento.
    pub codigo: &'static str,
    /// O nome.
    pub nome: &'static str,
    /// Receita ou despesa.
    pub natureza: Natureza,
    /// O papel, só nas contas de receita que a DRE deve agrupar.
    pub papel: Option<PapelConta>,
}

const fn despesa(codigo: &'static str, nome: &'static str) -> ContaDeModelo {
    ContaDeModelo {
        codigo,
        nome,
        natureza: Natureza::Despesa,
        papel: None,
    }
}

const fn receita(codigo: &'static str, nome: &'static str, papel: PapelConta) -> ContaDeModelo {
    ContaDeModelo {
        codigo,
        nome,
        natureza: Natureza::Receita,
        papel: Some(papel),
    }
}

/// As despesas comuns a todo segmento.
const COMUNS: &[ContaDeModelo] = &[
    despesa("5.20", "Aluguel"),
    despesa("5.21", "Energia elétrica"),
    despesa("5.22", "Água"),
    despesa("5.23", "Internet e telefone"),
    despesa("5.24", "Pró-labore"),
    despesa("5.25", "Salários e encargos"),
    despesa("5.26", "Contador"),
    despesa("5.27", "Marketing"),
    despesa("5.28", "Sistemas e assinaturas"),
    despesa("5.29", "Manutenção"),
    despesa("5.30", "Transporte e combustível"),
    despesa("5.31", "Material de escritório e limpeza"),
    despesa("5.32", "Tarifas bancárias"),
    despesa("5.33", "Seguros"),
    despesa("5.34", "Impostos e taxas"),
];

const COMERCIO: &[ContaDeModelo] = &[
    despesa("5.40", "Fretes sobre compras"),
    despesa("5.41", "Embalagens"),
    despesa("5.42", "Comissões de vendedores"),
    despesa("5.43", "Taxas de marketplace"),
];

const SERVICOS: &[ContaDeModelo] = &[
    receita(
        "4.20",
        "Serviços recorrentes (mensalidades)",
        PapelConta::ReceitaServicos,
    ),
    receita("4.21", "Serviços avulsos", PapelConta::ReceitaServicos),
    despesa("5.40", "Subcontratação de terceiros"),
    despesa("5.41", "Viagens e hospedagem"),
];

const ASSISTENCIA: &[ContaDeModelo] = &[
    receita("4.22", "Serviços de bancada", PapelConta::ReceitaServicos),
    receita(
        "4.23",
        "Venda de peças e acessórios",
        PapelConta::ReceitaVendas,
    ),
    despesa("5.40", "Peças e componentes para reparo"),
    despesa("5.41", "Ferramentas e consumíveis (solda, fluxo)"),
    despesa("5.42", "Garantia e retrabalho"),
    despesa("5.43", "Laboratório terceirizado"),
];

/// As contas que o modelo acrescenta ao plano base (as comuns primeiro).
#[must_use]
pub fn contas_do_modelo(modelo: ModeloDePlano) -> Vec<ContaDeModelo> {
    let proprias = match modelo {
        ModeloDePlano::Comercio => COMERCIO,
        ModeloDePlano::Servicos => SERVICOS,
        ModeloDePlano::AssistenciaTecnica => ASSISTENCIA,
    };
    COMUNS.iter().chain(proprias).copied().collect()
}

#[cfg(test)]
mod testes {
    use std::collections::HashSet;

    use super::*;
    use crate::plano::plano_padrao;

    #[test]
    fn nenhum_modelo_colide_com_o_plano_base_nem_consigo_mesmo() {
        let base: HashSet<String> = plano_padrao()
            .iter()
            .map(|c| c.codigo.como_str().to_owned())
            .collect();
        for m in ModeloDePlano::TODOS {
            let contas = contas_do_modelo(m);
            let mut vistos = HashSet::new();
            for c in &contas {
                assert!(
                    !base.contains(c.codigo),
                    "{m:?}: {} já existe no plano base",
                    c.codigo
                );
                assert!(
                    vistos.insert(c.codigo),
                    "{m:?}: código repetido {}",
                    c.codigo
                );
                // O pai (sem o último segmento) é uma sintética do plano base.
                let pai = &c.codigo[..c.codigo.rfind('.').unwrap()];
                assert!(
                    base.contains(pai),
                    "{m:?}: pai {pai} de {} não existe",
                    c.codigo
                );
                let esperado = if c.codigo.starts_with('4') {
                    Natureza::Receita
                } else {
                    Natureza::Despesa
                };
                assert_eq!(c.natureza, esperado, "{}", c.codigo);
            }
        }
    }

    #[test]
    fn receitas_do_modelo_levam_papel_e_despesas_nao() {
        for m in ModeloDePlano::TODOS {
            for c in contas_do_modelo(m) {
                assert_eq!(
                    c.papel.is_some(),
                    c.natureza == Natureza::Receita,
                    "{}",
                    c.codigo
                );
            }
        }
        assert!(contas_do_modelo(ModeloDePlano::Comercio)
            .iter()
            .any(|c| c.nome == "Aluguel"));
    }
}
