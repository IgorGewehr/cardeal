//! Importação da estrutura mercadológica por planilha (CSV): domínio puro.
//!
//! Cada linha é um **caminho** — `Grupo;Subgrupo;Categoria;Subcategoria` (os níveis de baixo
//! são opcionais). O separador pode ser `;` ou `,` e o cabeçalho é opcional. [`planejar`]
//! compara o caminho com a árvore que já existe (por nome, sem caixa nem acento, sob o mesmo
//! pai) e devolve o que precisa ser criado, o que já existia e os erros por linha. Nada é
//! gravado aqui, e quem aplica recusa o plano inteiro se houver um único erro: nunca grava
//! parcialmente. Importar o mesmo arquivo duas vezes não cria nada na segunda.

use std::collections::{HashMap, HashSet};

use cardeal_kernel::texto::{chave_busca, sem_acentos};
use cardeal_kernel::Id;
use serde::{Deserialize, Serialize};

use crate::hierarquia::{nome_do_nivel, NIVEL_MAXIMO};

/// Quantos caminhos novos a prévia lista (o resto só conta).
pub const AMOSTRA_DA_PREVIA: usize = 200;

/// Um grupo que já está cadastrado.
#[derive(Debug, Clone)]
pub struct NoExistente {
    /// O grupo.
    pub id: Id,
    /// O código.
    pub codigo: String,
    /// O nome.
    pub nome: String,
    /// O pai; `None` na raiz.
    pub pai: Option<Id>,
}

/// O pai de um nó a criar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PaiPlanejado {
    /// Raiz (nível 1).
    Raiz,
    /// Um grupo que já existe.
    Existente(Id),
    /// Outro nó do próprio plano (índice em [`Plano::nos`]); sempre anterior ao filho.
    Novo(usize),
}

/// Um grupo a criar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoNovo {
    /// O código gerado (único na empresa).
    pub codigo: String,
    /// O nome, como veio na planilha.
    pub nome: String,
    /// O pai.
    pub pai: PaiPlanejado,
    /// O nível (1 = Grupo).
    pub nivel: u8,
    /// O caminho completo para exibição ("Bebidas › Refrigerantes").
    pub caminho: String,
}

/// Um problema numa linha da planilha.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErroDeLinha {
    /// O número da linha no arquivo (1 = primeira).
    pub linha: usize,
    /// O que está errado.
    pub mensagem: String,
}

/// O resultado da análise da planilha.
#[derive(Debug, Clone, Default)]
pub struct Plano {
    /// Os grupos a criar, pais antes dos filhos.
    pub nos: Vec<NoNovo>,
    /// Quantos grupos distintos da planilha já existiam.
    pub ja_existiam: u32,
    /// Quantas linhas com caminho a planilha trouxe.
    pub linhas: u32,
    /// Os erros por linha; com qualquer um, nada é gravado.
    pub erros: Vec<ErroDeLinha>,
}

/// O que a tela mostra antes de confirmar.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PreviaImportacao {
    /// Quantos grupos serão criados.
    pub novos: u32,
    /// Quantos já existiam.
    pub ja_existiam: u32,
    /// Quantas linhas com caminho.
    pub linhas: u32,
    /// Os erros por linha.
    pub erros: Vec<ErroDeLinha>,
    /// Os primeiros caminhos a criar (até [`AMOSTRA_DA_PREVIA`]).
    pub amostra: Vec<String>,
}

impl Plano {
    /// O resumo para a tela.
    #[must_use]
    pub fn previa(&self) -> PreviaImportacao {
        PreviaImportacao {
            novos: u32::try_from(self.nos.len()).unwrap_or(u32::MAX),
            ja_existiam: self.ja_existiam,
            linhas: self.linhas,
            erros: self.erros.clone(),
            amostra: self
                .nos
                .iter()
                .take(AMOSTRA_DA_PREVIA)
                .map(|n| n.caminho.clone())
                .collect(),
        }
    }
}

/// Onde um nome da planilha já está: na árvore existente ou entre os nós novos.
#[derive(Clone, Copy)]
enum Alvo {
    Existente(Id),
    Novo(usize),
}

/// O leitor de planilha é o do kernel (`;` ou `,`, aspas, BOM do Excel); usado também pela
/// importação de produtos ([`crate::importacao_produtos`]).
pub(crate) use cardeal_kernel::planilha::ler_linhas;

/// A primeira linha é cabeçalho quando a primeira célula é o nome do nível 1.
fn e_cabecalho(celulas: &[String]) -> bool {
    celulas.first().is_some_and(|c| {
        let k = chave_busca(c);
        k == "grupo" || k == "nivel 1"
    })
}

/// O código sugerido para um nome: letras e números em maiúsculas, até 6 caracteres.
fn codigo_base(nome: &str) -> String {
    let c: String = sem_acentos(nome)
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .take(6)
        .collect::<String>()
        .to_uppercase();
    if c.is_empty() {
        "GRP".to_owned()
    } else {
        c
    }
}

/// Confere um caminho já sem as células vazias do fim: no máximo [`NIVEL_MAXIMO`] níveis e
/// nenhum nível vazio no meio. Devolve a mensagem para o usuário.
pub(crate) fn validar_caminho(cels: &[String]) -> Result<(), String> {
    if cels.len() > usize::from(NIVEL_MAXIMO) {
        return Err(format!(
            "{} níveis; a estrutura aceita no máximo {NIVEL_MAXIMO} (até {})",
            cels.len(),
            nome_do_nivel(NIVEL_MAXIMO)
        ));
    }
    if let Some(i) = cels.iter().position(String::is_empty) {
        return Err(format!(
            "o nível {} está vazio, mas há níveis abaixo dele",
            nome_do_nivel(u8::try_from(i + 1).unwrap_or(1))
        ));
    }
    Ok(())
}

/// Resolve caminhos contra a árvore existente, acumulando os grupos a criar — o miolo de
/// [`planejar`], usado também pela importação de produtos para achar (ou criar) o grupo de
/// cada produto.
pub(crate) struct ResolvedorDeCaminhos {
    /// (pai, chave do nome) → nó; existentes e novos na mesma tabela.
    por_pai: HashMap<(PaiPlanejado, String), Alvo>,
    codigos: HashSet<String>,
    usados_existentes: HashSet<Id>,
    nos: Vec<NoNovo>,
}

impl ResolvedorDeCaminhos {
    pub(crate) fn novo(existentes: &[NoExistente]) -> Self {
        let mut por_pai = HashMap::new();
        for e in existentes {
            let pai = e.pai.map_or(PaiPlanejado::Raiz, PaiPlanejado::Existente);
            por_pai
                .entry((pai, chave_busca(&e.nome)))
                .or_insert(Alvo::Existente(e.id));
        }
        Self {
            por_pai,
            codigos: existentes.iter().map(|e| e.codigo.to_uppercase()).collect(),
            usados_existentes: HashSet::new(),
            nos: Vec::new(),
        }
    }

    /// Acha ou planeja cada nível de um caminho já validado ([`validar_caminho`], não vazio)
    /// e devolve o nó da ponta — o grupo do produto, na importação de produtos.
    pub(crate) fn resolver(&mut self, cels: &[String]) -> PaiPlanejado {
        let mut pai = PaiPlanejado::Raiz;
        let mut caminho: Vec<&str> = Vec::new();
        for (i, nome) in cels.iter().enumerate() {
            caminho.push(nome);
            let chave = (pai, chave_busca(nome));
            match self.por_pai.get(&chave).copied() {
                Some(Alvo::Existente(id)) => {
                    self.usados_existentes.insert(id);
                    pai = PaiPlanejado::Existente(id);
                }
                Some(Alvo::Novo(n)) => pai = PaiPlanejado::Novo(n),
                None => {
                    let base = codigo_base(nome);
                    let mut codigo = base.clone();
                    let mut n = 2u32;
                    while self.codigos.contains(&codigo) {
                        codigo = format!("{base}{n}");
                        n += 1;
                    }
                    self.codigos.insert(codigo.clone());
                    let indice = self.nos.len();
                    self.nos.push(NoNovo {
                        codigo,
                        nome: nome.clone(),
                        pai,
                        nivel: u8::try_from(i + 1).unwrap_or(NIVEL_MAXIMO),
                        caminho: caminho.join(" › "),
                    });
                    self.por_pai.insert(chave, Alvo::Novo(indice));
                    pai = PaiPlanejado::Novo(indice);
                }
            }
        }
        pai
    }

    /// Os grupos a criar (pais antes dos filhos) e quantos existentes foram usados.
    pub(crate) fn concluir(self) -> (Vec<NoNovo>, u32) {
        (
            self.nos,
            u32::try_from(self.usados_existentes.len()).unwrap_or(u32::MAX),
        )
    }
}

/// Analisa a planilha contra a árvore existente.
#[must_use]
pub fn planejar(texto: &str, existentes: &[NoExistente]) -> Plano {
    let mut plano = Plano::default();
    let mut linhas = ler_linhas(texto);
    if linhas.first().is_some_and(|(_, c)| e_cabecalho(c)) {
        linhas.remove(0);
    }

    let mut resolvedor = ResolvedorDeCaminhos::novo(existentes);
    for (linha, mut cels) in linhas {
        while cels.last().is_some_and(String::is_empty) {
            cels.pop();
        }
        if cels.is_empty() {
            continue; // só separadores
        }
        if let Err(mensagem) = validar_caminho(&cels) {
            plano.erros.push(ErroDeLinha { linha, mensagem });
            continue;
        }
        plano.linhas += 1;
        // Aqui nada mais pode falhar: a linha é válida e os efeitos valem direto.
        resolvedor.resolver(&cels);
    }
    (plano.nos, plano.ja_existiam) = resolvedor.concluir();
    plano
}

#[cfg(test)]
mod testes {
    use super::*;

    fn existente(nome: &str, codigo: &str, pai: Option<Id>) -> NoExistente {
        NoExistente {
            id: Id::novo(),
            codigo: codigo.into(),
            nome: nome.into(),
            pai,
        }
    }

    /// Aplica o plano numa árvore em memória, como o comando faz no banco.
    fn aplicar(plano: &Plano, arvore: &mut Vec<NoExistente>) {
        let mut ids: Vec<Id> = Vec::new();
        for n in &plano.nos {
            let id = Id::novo();
            ids.push(id);
            let pai = match n.pai {
                PaiPlanejado::Raiz => None,
                PaiPlanejado::Existente(p) => Some(p),
                PaiPlanejado::Novo(i) => Some(ids[i]),
            };
            arvore.push(NoExistente {
                id,
                codigo: n.codigo.clone(),
                nome: n.nome.clone(),
                pai,
            });
        }
    }

    #[test]
    fn caminho_vira_nos_com_pais_antes_dos_filhos() {
        let p = planejar("Bebidas;Refrigerantes;Cola\nBebidas;Sucos\n", &[]);
        assert!(p.erros.is_empty());
        let nomes: Vec<_> = p.nos.iter().map(|n| n.nome.as_str()).collect();
        assert_eq!(nomes, ["Bebidas", "Refrigerantes", "Cola", "Sucos"]);
        assert_eq!(p.nos[2].pai, PaiPlanejado::Novo(1));
        assert_eq!(p.nos[3].pai, PaiPlanejado::Novo(0));
        assert_eq!(p.nos[2].caminho, "Bebidas › Refrigerantes › Cola");
        assert_eq!(p.nos[2].nivel, 3);
        assert_eq!(p.linhas, 2);
    }

    #[test]
    fn virgula_cabecalho_aspas_e_bom() {
        let p = planejar("\u{feff}Grupo,Subgrupo\n\"Pecas, acessorios\",Telas\n", &[]);
        assert!(p.erros.is_empty());
        assert_eq!(p.nos[0].nome, "Pecas, acessorios");
        assert_eq!(p.nos.len(), 2);
        assert_eq!(p.linhas, 1);
    }

    #[test]
    fn ignora_o_que_existe_sem_caixa_nem_acento() {
        let bebidas = existente("Bebidas", "BEB", None);
        let pecas = existente("Peças", "PEC", None);
        let id_bebidas = bebidas.id;
        let p = planejar("BEBIDAS;Sucos\npecas\n", &[bebidas, pecas]);
        assert_eq!(p.ja_existiam, 2);
        assert_eq!(p.nos.len(), 1);
        assert_eq!(p.nos[0].pai, PaiPlanejado::Existente(id_bebidas));
    }

    #[test]
    fn mesmo_nome_em_pais_diferentes_nao_e_duplicata() {
        let p = planejar("A;Outros\nB;Outros\n", &[]);
        assert_eq!(p.nos.len(), 4);
    }

    #[test]
    fn duplicata_na_planilha_cria_uma_vez() {
        let p = planejar("A;B\nA;B\na;b;C\n", &[]);
        assert_eq!(p.nos.len(), 3);
        assert_eq!(p.linhas, 3);
    }

    #[test]
    fn erros_por_linha_sem_gravar_nada_da_linha() {
        let p = planejar("Ok;Sub\nA;;C\n;B\nA;B;C;D;E\nValido\n", &[]);
        assert_eq!(p.erros.len(), 3);
        assert_eq!(p.erros[0].linha, 2);
        assert!(p.erros[0].mensagem.contains("Subgrupo"));
        assert_eq!(p.erros[1].linha, 3);
        assert!(p.erros[1].mensagem.contains("Grupo"));
        assert_eq!(p.erros[2].linha, 4);
        assert!(p.erros[2].mensagem.contains('5'));
        // As linhas boas entram no plano; as ruins não deixam nó nenhum.
        let nomes: Vec<_> = p.nos.iter().map(|n| n.nome.as_str()).collect();
        assert_eq!(nomes, ["Ok", "Sub", "Valido"]);
    }

    #[test]
    fn linhas_em_branco_e_so_separadores_sao_ignoradas() {
        let p = planejar("\n;;;\nA\n\n", &[]);
        assert!(p.erros.is_empty());
        assert_eq!(p.nos.len(), 1);
    }

    #[test]
    fn importar_duas_vezes_nao_cria_nada_na_segunda() {
        let csv = "Grupo;Subgrupo;Categoria\nBebidas;Refrigerantes;Cola\nPeças;Telas\n";
        let mut arvore = Vec::new();
        let p1 = planejar(csv, &arvore);
        assert_eq!(p1.nos.len(), 5);
        aplicar(&p1, &mut arvore);
        let p2 = planejar(csv, &arvore);
        assert!(p2.nos.is_empty());
        assert_eq!(p2.ja_existiam, 5);
    }

    #[test]
    fn codigos_gerados_sao_unicos_e_sem_acento() {
        let p = planejar(
            "Peças;Pecas Usadas\nPeças Novas\n",
            &[existente("X", "PECAS", None)],
        );
        let codigos: Vec<_> = p.nos.iter().map(|n| n.codigo.as_str()).collect();
        assert_eq!(codigos, ["PECAS2", "PECASU", "PECASN"]);
        let unicos: HashSet<_> = codigos.iter().collect();
        assert_eq!(unicos.len(), codigos.len());
    }

    #[test]
    fn previa_resume() {
        let p = planejar("A;B\n;X\n", &[]);
        let v = p.previa();
        assert_eq!((v.novos, v.ja_existiam, v.erros.len()), (2, 0, 1));
        assert_eq!(v.amostra, ["A", "A › B"]);
    }
}
