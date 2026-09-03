//! Deduplicação assistida — por documento (certeza) e por similaridade de nome (sugestão).
//!
//! `docs/modulos/clientes.md` §11.1: a `UNIQUE(empresa, tipo, numero)` do documento é a
//! garantia física; a similaridade de nome **nunca bloqueia**, só sugere conferência —
//! nomes fantasia legitimamente se parecem. Domínio puro (usa `cardeal_kernel::texto`).

use cardeal_kernel::{texto, Id};

/// Limiar de similaridade de nome acima do qual vale sugerir conferência
/// (`docs/modulos/clientes.md` §5, "trigrama ≥ 0,85").
pub const LIMIAR_SIMILARIDADE: f32 = 0.85;

/// Um candidato a duplicata: o mínimo que o domínio precisa de uma pessoa já cadastrada.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidato {
    /// O id da pessoa existente.
    pub pessoa: Id,
    /// O nome/razão social cadastrado.
    pub nome: String,
    /// Documentos canônicos (só dígitos) já vinculados à pessoa.
    pub documentos: Vec<String>,
}

/// A força de uma sugestão de duplicidade.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForcaDuplicidade {
    /// Documento idêntico — é a mesma pessoa, quase com certeza.
    DocumentoIgual,
    /// Nomes muito parecidos — vale a conferência humana.
    NomeParecido,
}

/// Uma sugestão de que o cadastro em andamento pode já existir.
#[derive(Debug, Clone, PartialEq)]
pub struct SugestaoDuplicidade {
    /// A pessoa candidata.
    pub pessoa: Id,
    /// O nome dela, para exibir na tela de conferência.
    pub nome: String,
    /// Por que foi sugerida.
    pub forca: ForcaDuplicidade,
    /// A similaridade de nome apurada (0–1), útil para ordenar a lista.
    pub similaridade: f32,
}

/// Procura duplicatas do cadastro `(nome, documentos)` entre `candidatos`.
///
/// Um documento em comum gera [`ForcaDuplicidade::DocumentoIgual`] (ordenado primeiro);
/// senão, um nome com similaridade `≥ LIMIAR_SIMILARIDADE` gera
/// [`ForcaDuplicidade::NomeParecido`]. A lista sai ordenada da sugestão mais forte para a
/// mais fraca.
#[must_use]
pub fn sugerir_duplicatas(
    nome: &str,
    documentos: &[String],
    candidatos: &[Candidato],
) -> Vec<SugestaoDuplicidade> {
    let alvo = texto::chave_busca(nome);
    let docs_alvo: Vec<&str> = documentos
        .iter()
        .map(|d| d.trim())
        .filter(|d| !d.is_empty())
        .collect();

    let mut sugestoes: Vec<SugestaoDuplicidade> = candidatos
        .iter()
        .filter_map(|c| {
            let sim = texto::similaridade(&alvo, &texto::chave_busca(&c.nome));
            let doc_igual = c
                .documentos
                .iter()
                .any(|d| docs_alvo.iter().any(|a| *a == d.trim()));
            if doc_igual {
                Some(SugestaoDuplicidade {
                    pessoa: c.pessoa,
                    nome: c.nome.clone(),
                    forca: ForcaDuplicidade::DocumentoIgual,
                    similaridade: sim,
                })
            } else if sim >= LIMIAR_SIMILARIDADE {
                Some(SugestaoDuplicidade {
                    pessoa: c.pessoa,
                    nome: c.nome.clone(),
                    forca: ForcaDuplicidade::NomeParecido,
                    similaridade: sim,
                })
            } else {
                None
            }
        })
        .collect();

    sugestoes.sort_by(|a, b| {
        let peso = |f| match f {
            ForcaDuplicidade::DocumentoIgual => 1,
            ForcaDuplicidade::NomeParecido => 0,
        };
        peso(b.forca)
            .cmp(&peso(a.forca))
            .then(b.similaridade.total_cmp(&a.similaridade))
    });
    sugestoes
}

#[cfg(test)]
mod testes {
    use super::*;

    fn candidato(nome: &str, docs: &[&str]) -> Candidato {
        Candidato {
            pessoa: Id::novo(),
            nome: nome.to_string(),
            documentos: docs.iter().map(|s| (*s).to_string()).collect(),
        }
    }

    #[test]
    fn documento_igual_vence_e_vem_primeiro() {
        let candidatos = vec![
            candidato("Padaria do Zé", &["11222333000181"]),
            candidato("Mercado Bom Preço LTDA", &["99888777000166"]),
        ];
        let s = sugerir_duplicatas(
            "Mercearia Diferente",
            &["11222333000181".to_string()],
            &candidatos,
        );
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].forca, ForcaDuplicidade::DocumentoIgual);
        assert_eq!(s[0].nome, "Padaria do Zé");
    }

    #[test]
    fn nome_muito_parecido_gera_sugestao_fraca() {
        let candidatos = vec![candidato("Mercado Bom Preço LTDA", &["99888777000166"])];
        let s = sugerir_duplicatas("Mercado Bom Preço Ltda", &[], &candidatos);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].forca, ForcaDuplicidade::NomeParecido);
        assert!(s[0].similaridade >= LIMIAR_SIMILARIDADE);
    }

    #[test]
    fn nome_diferente_sem_documento_nao_sugere_nada() {
        let candidatos = vec![candidato("Auto Peças Silva", &["12345678000195"])];
        let s = sugerir_duplicatas("Restaurante da Praça", &[], &candidatos);
        assert!(s.is_empty());
    }
}
