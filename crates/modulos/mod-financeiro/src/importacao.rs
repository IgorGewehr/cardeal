//! Importação das contas em aberto (a receber e a pagar) por planilha CSV — a implantação do
//! primeiro dia: o que os clientes devem e o que a empresa deve, vindo do sistema antigo.
//!
//! Cada linha é **uma conta** (um título de uma parcela). A primeira linha é o cabeçalho; as
//! colunas valem pelo nome, em qualquer ordem (sem caixa nem acento):
//! - `Tipo` (obrigatória): `Receber` ou `Pagar` (também `R`/`P`, `Cliente`/`Fornecedor`).
//! - `Vencimento` e `Valor` (obrigatórias): o valor **em aberto**, maior que zero.
//! - `Nome` (ou `Cliente`, `Fornecedor`, `Pessoa`) e `CPF/CNPJ`: de quem é a conta. A pessoa é
//!   achada pelo documento ou, sem ele, pelo nome; quem não está no cadastro é criado (com o
//!   nome e o documento). Sem nenhum dos dois, a conta fica sem pessoa (como o título avulso).
//! - `Emissão` (padrão: o vencimento ou hoje, o que vier antes), `Número` (do documento, da
//!   nota ou do boleto), `Categoria` e `Descrição`: vão para a observação do título.
//!
//! [`planejar`] só lê e confere; quem resolve as pessoas e grava é o comando. Cada linha ganha
//! uma **chave** estável (tipo, pessoa, vencimento, valor, observação e a ocorrência na
//! planilha), gravada como origem do título: importar o mesmo arquivo de novo não duplica.

use std::collections::HashMap;

use cardeal_kernel::documento::Documento;
use cardeal_kernel::planilha::{chave_coluna, ler_linhas};
use cardeal_kernel::texto::chave_busca;
use cardeal_kernel::{Data, Dinheiro, Id};
use serde::{Deserialize, Serialize};

use crate::titulo::EspecieTitulo;

/// Quantas contas a prévia mostra.
pub const AMOSTRA_DA_PREVIA: usize = 200;

/// O módulo de origem dos títulos importados.
pub const ORIGEM_IMPORTACAO: &str = "importacao";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Coluna {
    Tipo,
    Nome,
    Documento,
    Vencimento,
    Valor,
    Emissao,
    Numero,
    Categoria,
    Descricao,
}

fn coluna_de(titulo: &str) -> Option<Coluna> {
    Some(match chave_coluna(titulo).as_str() {
        "tipo" | "natureza" | "receberpagar" | "pagarreceber" => Coluna::Tipo,
        "nome" | "cliente" | "fornecedor" | "pessoa" | "contraparte" | "razaosocial" | "sacado"
        | "favorecido" | "clientefornecedor" => Coluna::Nome,
        "cpfcnpj" | "cnpjcpf" | "cpf" | "cnpj" | "cpfoucnpj" => Coluna::Documento,
        "vencimento" | "datadevencimento" | "datavencimento" | "venc" | "data" => {
            Coluna::Vencimento
        }
        "valor" | "valoremaberto" | "saldo" | "valorrestante" | "emaberto" | "total" => {
            Coluna::Valor
        }
        "emissao" | "datadeemissao" | "dataemissao" => Coluna::Emissao,
        "numero" | "numerododocumento" | "numerodocumento" | "nodocumento" | "ndocumento"
        | "nrodocumento" | "nrdocumento" | "documento" | "doc" | "nota" | "notafiscal" | "nf"
        | "boleto" | "duplicata" => Coluna::Numero,
        "categoria" | "conta" | "planodecontas" => Coluna::Categoria,
        "descricao" | "historico" | "observacao" | "obs" => Coluna::Descricao,
        _ => return None,
    })
}

/// Uma conta lida da planilha.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContaDaPlanilha {
    /// A linha no arquivo.
    pub linha: usize,
    /// A receber ou a pagar.
    pub especie: EspecieTitulo,
    /// O nome da pessoa.
    pub nome: Option<String>,
    /// O CPF/CNPJ (só dígitos, já conferido).
    pub documento: Option<String>,
    /// O vencimento.
    pub vencimento: Data,
    /// A emissão.
    pub emissao: Data,
    /// O valor em aberto.
    pub valor: Dinheiro,
    /// Número, categoria e descrição, juntos.
    pub observacao: Option<String>,
    /// A chave estável da linha (a origem do título).
    pub chave: Id,
}

impl ContaDaPlanilha {
    /// Quem é a pessoa da conta, para agrupar as linhas da mesma pessoa: o documento ou, sem
    /// ele, o nome normalizado.
    #[must_use]
    pub fn chave_pessoa(&self) -> Option<String> {
        self.documento
            .clone()
            .or_else(|| self.nome.as_deref().map(chave_busca))
            .filter(|c| !c.is_empty())
    }
}

/// Um erro numa linha.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErroDeLinha {
    /// A linha no arquivo.
    pub linha: usize,
    /// O que está errado.
    pub mensagem: String,
}

/// O que a planilha traz.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanilhaDeContas {
    /// As contas lidas.
    pub contas: Vec<ContaDaPlanilha>,
    /// Os erros por linha.
    pub erros: Vec<ErroDeLinha>,
    /// As colunas que a importação não conhece.
    pub colunas_ignoradas: Vec<String>,
}

/// Uma linha da prévia.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinhaPreviaConta {
    /// "Receber" ou "Pagar".
    pub tipo: String,
    /// O nome da pessoa ("" sem pessoa); "(novo)" no fim quando será cadastrada.
    pub pessoa: String,
    /// O vencimento.
    pub vencimento: Data,
    /// O valor.
    pub valor: Dinheiro,
    /// A observação.
    pub observacao: String,
}

/// O que a tela mostra antes de confirmar.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreviaImportacaoContas {
    /// Contas a receber a criar.
    pub a_receber: u32,
    /// A soma delas.
    pub total_a_receber: Dinheiro,
    /// Contas a pagar a criar.
    pub a_pagar: u32,
    /// A soma delas.
    pub total_a_pagar: Dinheiro,
    /// Quantas já tinham sido importadas.
    pub ja_existiam: u32,
    /// Quantas das novas já estão vencidas.
    pub vencidas: u32,
    /// As pessoas que serão cadastradas.
    pub pessoas_novas: Vec<String>,
    /// Os erros por linha.
    pub erros: Vec<ErroDeLinha>,
    /// As colunas que ficam de fora.
    pub colunas_ignoradas: Vec<String>,
    /// As primeiras contas a criar (até [`AMOSTRA_DA_PREVIA`]).
    pub amostra: Vec<LinhaPreviaConta>,
}

fn celula(cels: &[String], cab: &HashMap<Coluna, usize>, c: Coluna) -> Option<String> {
    cab.get(&c)
        .and_then(|&i| cels.get(i))
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
}

fn ler_especie(texto: &str) -> Result<EspecieTitulo, String> {
    let t = chave_coluna(texto);
    if matches!(t.as_str(), "r" | "cr" | "cliente" | "entrada" | "credito")
        || t.ends_with("receber")
        || t.starts_with("receb")
    {
        Ok(EspecieTitulo::Receber)
    } else if matches!(
        t.as_str(),
        "p" | "cp" | "fornecedor" | "saida" | "debito" | "despesa"
    ) || t.ends_with("pagar")
        || t.starts_with("pag")
    {
        Ok(EspecieTitulo::Pagar)
    } else {
        Err(format!("Tipo \"{texto}\": use Receber ou Pagar"))
    }
}

fn ler_data(texto: &str, campo: &str) -> Result<Data, String> {
    texto
        .parse::<Data>()
        .map_err(|_| format!("{campo} inválido: {texto}"))
}

/// FNV-1a de 128 bits: estável entre versões e máquinas (o `Hash` da biblioteca padrão não
/// promete isso), que é o que a chave de origem precisa.
fn fnv128(texto: &str) -> [u8; 16] {
    const BASE: u128 = 0x6c62_272e_07bb_0142_62b8_2175_6295_c58d;
    const PRIMO: u128 = 0x0000_0000_0100_0000_0000_0000_0000_013b;
    texto
        .bytes()
        .fold(BASE, |h, b| (h ^ u128::from(b)).wrapping_mul(PRIMO))
        .to_be_bytes()
}

fn ler_conta(
    linha: usize,
    cels: &[String],
    cab: &HashMap<Coluna, usize>,
    hoje: Data,
) -> Result<ContaDaPlanilha, String> {
    let get = |c| celula(cels, cab, c);
    let especie = ler_especie(&get(Coluna::Tipo).ok_or("falta o tipo (Receber ou Pagar)")?)?;
    let vencimento = ler_data(
        &get(Coluna::Vencimento).ok_or("falta o vencimento")?,
        "vencimento",
    )?;
    let bruto = get(Coluna::Valor).ok_or("falta o valor")?;
    let valor = Dinheiro::de_str(&bruto)
        .ok()
        .filter(|v| v.e_positivo())
        .ok_or(format!(
            "valor inválido (precisa ser maior que zero): {bruto}"
        ))?;
    let emissao = match get(Coluna::Emissao) {
        Some(e) => ler_data(&e, "emissão")?,
        None => hoje.min(vencimento),
    };
    if emissao > vencimento {
        return Err("a emissão é depois do vencimento".to_owned());
    }
    let documento = get(Coluna::Documento)
        .map(|d| {
            Documento::novo(&d)
                .map(|doc| doc.sem_mascara())
                .map_err(|_| format!("CPF/CNPJ inválido: {d}"))
        })
        .transpose()?;
    let numero = get(Coluna::Numero).map(|n| format!("Doc. {n}"));
    let partes: Vec<String> = [numero, get(Coluna::Categoria), get(Coluna::Descricao)]
        .into_iter()
        .flatten()
        .collect();
    Ok(ContaDaPlanilha {
        linha,
        especie,
        nome: get(Coluna::Nome),
        documento,
        vencimento,
        emissao,
        valor,
        observacao: (!partes.is_empty()).then(|| partes.join(" — ")),
        chave: Id::NULO, // preenchida em `planejar`, que conhece as repetições
    })
}

/// Lê e confere a planilha (sem tocar no banco). `hoje` é a data da implantação.
#[must_use]
pub fn planejar(texto: &str, hoje: Data) -> PlanilhaDeContas {
    let mut plano = PlanilhaDeContas::default();
    let mut linhas = ler_linhas(texto).into_iter();
    let Some((linha_cab, titulos)) = linhas.next() else {
        return plano;
    };
    let mut cab = HashMap::new();
    for (i, t) in titulos.iter().enumerate() {
        match coluna_de(t) {
            Some(c) => {
                if cab.insert(c, i).is_some() {
                    plano.erros.push(ErroDeLinha {
                        linha: linha_cab,
                        mensagem: format!("a coluna \"{t}\" aparece duas vezes"),
                    });
                }
            }
            None if !t.is_empty() => plano.colunas_ignoradas.push(t.clone()),
            None => {}
        }
    }
    let faltam: Vec<&str> = [
        (Coluna::Tipo, "Tipo"),
        (Coluna::Vencimento, "Vencimento"),
        (Coluna::Valor, "Valor"),
    ]
    .into_iter()
    .filter(|(c, _)| !cab.contains_key(c))
    .map(|(_, n)| n)
    .collect();
    if !faltam.is_empty() {
        plano.erros.push(ErroDeLinha {
            linha: linha_cab,
            mensagem: format!("faltam colunas no cabeçalho: {}", faltam.join(", ")),
        });
        return plano;
    }
    let mut ocorrencias: HashMap<String, u32> = HashMap::new();
    for (linha, cels) in linhas {
        match ler_conta(linha, &cels, &cab, hoje) {
            Err(mensagem) => plano.erros.push(ErroDeLinha { linha, mensagem }),
            Ok(mut c) => {
                let base = format!(
                    "{}|{}|{}|{}|{}",
                    match c.especie {
                        EspecieTitulo::Receber => "R",
                        EspecieTitulo::Pagar => "P",
                    },
                    c.chave_pessoa().unwrap_or_default(),
                    c.vencimento.formatar(),
                    c.valor.em_centavos(),
                    chave_busca(c.observacao.as_deref().unwrap_or_default()),
                );
                let n = ocorrencias.entry(base.clone()).or_default();
                *n += 1;
                c.chave = Id::de_bytes(fnv128(&format!("{base}|{n}")));
                plano.contas.push(c);
            }
        }
    }
    plano
}

/// De quem é a conta, já resolvido contra o cadastro.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PessoaDaConta {
    /// Sem pessoa.
    Nenhuma,
    /// Uma pessoa do cadastro.
    Existente(Id),
    /// A n-ésima pessoa nova (ver [`ContasResolvidas::pessoas_novas`]).
    Nova(usize),
}

/// Uma pessoa a cadastrar: o nome e o documento (só dígitos).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PessoaNova {
    /// O nome.
    pub nome: String,
    /// O CPF/CNPJ.
    pub documento: Option<String>,
    /// A espécie da primeira conta dela (define o papel: cliente ou fornecedor).
    pub especie: EspecieTitulo,
}

/// A planilha com as pessoas resolvidas e o que já foi importado separado.
#[derive(Debug, Clone, Default)]
pub struct ContasResolvidas {
    /// As contas a criar e de quem são.
    pub novas: Vec<(ContaDaPlanilha, PessoaDaConta)>,
    /// As pessoas a cadastrar.
    pub pessoas_novas: Vec<PessoaNova>,
    /// Quantas já tinham sido importadas.
    pub ja_existiam: u32,
    /// Os erros por linha (os da leitura mais os de pessoa).
    pub erros: Vec<ErroDeLinha>,
    /// As colunas que ficam de fora.
    pub colunas_ignoradas: Vec<String>,
}

/// Resolve as pessoas (pelo documento ou, sem ele, pelo nome — nome repetido no cadastro é
/// erro, porque não dá para saber de quem é) e separa o que já foi importado.
#[must_use]
pub fn resolver(
    planilha: PlanilhaDeContas,
    pessoas: &mod_clientes::importacao::IndicePessoas,
    ja_importada: impl Fn(Id) -> bool,
) -> ContasResolvidas {
    let mut r = ContasResolvidas {
        erros: planilha.erros,
        colunas_ignoradas: planilha.colunas_ignoradas,
        ..ContasResolvidas::default()
    };
    let mut novas_por_chave: HashMap<String, usize> = HashMap::new();
    for conta in planilha.contas {
        if ja_importada(conta.chave) {
            r.ja_existiam += 1;
            continue;
        }
        let existente = match (&conta.documento, &conta.nome) {
            (Some(doc), _) => pessoas.do_documento(doc),
            (None, Some(nome)) => match pessoas.do_nome(nome) {
                [] => None,
                [unica] => Some(*unica),
                varias => {
                    r.erros.push(ErroDeLinha {
                        linha: conta.linha,
                        mensagem: format!(
                            "há {} cadastros com o nome \"{nome}\": informe o CPF/CNPJ",
                            varias.len()
                        ),
                    });
                    continue;
                }
            },
            (None, None) => None,
        };
        let pessoa = match (existente, conta.chave_pessoa()) {
            (Some(id), _) => PessoaDaConta::Existente(id),
            (None, None) => PessoaDaConta::Nenhuma,
            (None, Some(chave)) => {
                let Some(nome) = conta.nome.clone() else {
                    r.erros.push(ErroDeLinha {
                        linha: conta.linha,
                        mensagem: "o CPF/CNPJ não está no cadastro: informe também o nome"
                            .to_owned(),
                    });
                    continue;
                };
                let i = *novas_por_chave.entry(chave).or_insert_with(|| {
                    r.pessoas_novas.push(PessoaNova {
                        nome,
                        documento: conta.documento.clone(),
                        especie: conta.especie,
                    });
                    r.pessoas_novas.len() - 1
                });
                PessoaDaConta::Nova(i)
            }
        };
        r.novas.push((conta, pessoa));
    }
    r
}

impl ContasResolvidas {
    /// O resumo para a tela. `nomes` dá o nome das pessoas que já existem.
    #[must_use]
    pub fn previa(&self, hoje: Data, nomes: &HashMap<Id, String>) -> PreviaImportacaoContas {
        let mut p = PreviaImportacaoContas {
            ja_existiam: self.ja_existiam,
            pessoas_novas: self.pessoas_novas.iter().map(|n| n.nome.clone()).collect(),
            erros: self.erros.clone(),
            colunas_ignoradas: self.colunas_ignoradas.clone(),
            ..PreviaImportacaoContas::default()
        };
        for (c, pessoa) in &self.novas {
            match c.especie {
                EspecieTitulo::Receber => {
                    p.a_receber += 1;
                    p.total_a_receber += c.valor;
                }
                EspecieTitulo::Pagar => {
                    p.a_pagar += 1;
                    p.total_a_pagar += c.valor;
                }
            }
            p.vencidas += u32::from(c.vencimento < hoje);
            if p.amostra.len() < AMOSTRA_DA_PREVIA {
                p.amostra.push(LinhaPreviaConta {
                    tipo: match c.especie {
                        EspecieTitulo::Receber => "Receber",
                        EspecieTitulo::Pagar => "Pagar",
                    }
                    .to_owned(),
                    pessoa: match pessoa {
                        PessoaDaConta::Nenhuma => String::new(),
                        PessoaDaConta::Existente(id) => nomes.get(id).cloned().unwrap_or_default(),
                        PessoaDaConta::Nova(i) => format!("{} (novo)", self.pessoas_novas[*i].nome),
                    },
                    vencimento: c.vencimento,
                    valor: c.valor,
                    observacao: c.observacao.clone().unwrap_or_default(),
                });
            }
        }
        p
    }

    /// As pessoas que já existem e aparecem nas contas (para buscar os nomes).
    #[must_use]
    pub fn pessoas_existentes(&self) -> Vec<Id> {
        self.novas
            .iter()
            .filter_map(|(_, p)| match p {
                PessoaDaConta::Existente(id) => Some(*id),
                _ => None,
            })
            .collect()
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use mod_clientes::importacao::IndicePessoas;

    fn d(t: &str) -> Data {
        t.parse().unwrap()
    }

    const HOJE: &str = "16/10/2026";

    #[test]
    fn le_tipo_vencimento_valor_e_junta_a_observacao() {
        let csv = "Tipo;Cliente/Fornecedor;CPF/CNPJ;Vencimento;Valor;Nº documento;Descrição\n\
                   Receber;Ana Lima;529.982.247-25;10/11/2026;150,00;NF 12;Parcela 2/3\n\
                   A pagar;Distribuidora;;01/10/2026;R$ 1.200,50;;\n";
        let p = planejar(csv, d(HOJE));
        assert!(p.erros.is_empty(), "{:?}", p.erros);
        assert_eq!(p.contas.len(), 2);
        let ana = &p.contas[0];
        assert_eq!(ana.especie, EspecieTitulo::Receber);
        assert_eq!(ana.documento.as_deref(), Some("52998224725"));
        assert_eq!(ana.emissao, d(HOJE), "sem emissão: hoje, se vence depois");
        assert_eq!(ana.observacao.as_deref(), Some("Doc. NF 12 — Parcela 2/3"));
        let dist = &p.contas[1];
        assert_eq!(dist.especie, EspecieTitulo::Pagar);
        assert_eq!(dist.valor, Dinheiro::centavos(120_050));
        assert_eq!(
            dist.emissao,
            d("01/10/2026"),
            "vencida: emissão no vencimento"
        );
    }

    #[test]
    fn a_chave_e_estavel_e_separa_linhas_iguais() {
        let csv = "Tipo;Nome;Vencimento;Valor\n\
                   R;Ana;10/11/2026;50\n\
                   R;Ana;10/11/2026;50\n\
                   R;Bia;10/11/2026;50\n";
        let a = planejar(csv, d(HOJE));
        let b = planejar(csv, d("20/10/2026"));
        let chaves: Vec<Id> = a.contas.iter().map(|c| c.chave).collect();
        assert_eq!(chaves, b.contas.iter().map(|c| c.chave).collect::<Vec<_>>());
        assert_ne!(
            chaves[0], chaves[1],
            "duas cobranças iguais são duas contas"
        );
        assert_ne!(chaves[0], chaves[2]);
    }

    #[test]
    fn erros_por_linha_e_cabecalho_incompleto() {
        let csv = "Tipo;Nome;CPF;Vencimento;Valor;Emissão\n\
                   Talvez;Ana;;10/11/2026;50;\n\
                   R;Ana;;31/02/2026;50;\n\
                   R;Ana;;10/11/2026;0;\n\
                   R;Ana;123;10/11/2026;50;\n\
                   R;Ana;;10/11/2026;50;20/11/2026\n";
        let p = planejar(csv, d(HOJE));
        assert!(p.contas.is_empty());
        let linhas: Vec<usize> = p.erros.iter().map(|e| e.linha).collect();
        assert_eq!(linhas, vec![2, 3, 4, 5, 6], "{:?}", p.erros);
        let p = planejar("Nome;Valor\nAna;10\n", d(HOJE));
        assert!(
            p.erros[0].mensagem.contains("Tipo, Vencimento"),
            "{:?}",
            p.erros
        );
    }

    #[test]
    fn resolve_pessoas_por_documento_e_nome_e_pula_o_ja_importado() {
        let ana = Id::novo();
        let mut indice = IndicePessoas::default();
        indice.por_documento.insert("52998224725".into(), ana);
        indice
            .por_nome
            .insert(chave_busca("Zé"), vec![Id::novo(), Id::novo()]);
        indice.por_nome.insert(chave_busca("Bia"), vec![Id::novo()]);
        let csv = "Tipo;Nome;CPF/CNPJ;Vencimento;Valor\n\
                   R;Ana Lima;529.982.247-25;10/11/2026;50\n\
                   R;Bia;;10/11/2026;60\n\
                   P;Distribuidora Sul;11.222.333/0001-81;01/10/2026;70\n\
                   P;Distribuidora Sul;11.222.333/0001-81;01/11/2026;80\n\
                   R;Zé;;10/11/2026;90\n\
                   R;;;10/11/2026;100\n\
                   R;;390.533.447-05;10/11/2026;110\n\
                   R;Carlos;;10/11/2026;120\n";
        let planilha = planejar(csv, d(HOJE));
        let ja = planilha.contas[7].chave;
        let r = resolver(planilha, &indice, |c| c == ja);
        assert_eq!(r.ja_existiam, 1, "o Carlos já tinha sido importado");
        assert_eq!(r.pessoas_novas.len(), 1, "a distribuidora, uma vez só");
        assert_eq!(r.pessoas_novas[0].especie, EspecieTitulo::Pagar);
        let linhas: Vec<usize> = r.erros.iter().map(|e| e.linha).collect();
        assert_eq!(linhas, vec![6, 8], "Zé ambíguo; CPF novo sem nome");
        let pessoas: Vec<PessoaDaConta> = r.novas.iter().map(|(_, p)| *p).collect();
        assert_eq!(pessoas[0], PessoaDaConta::Existente(ana));
        assert!(matches!(pessoas[1], PessoaDaConta::Existente(_)));
        assert_eq!(pessoas[2], PessoaDaConta::Nova(0));
        assert_eq!(pessoas[3], PessoaDaConta::Nova(0));
        assert_eq!(pessoas[4], PessoaDaConta::Nenhuma);
        let previa = r.previa(d(HOJE), &HashMap::new());
        assert_eq!(
            (previa.a_receber, previa.a_pagar, previa.vencidas),
            (3, 2, 1)
        );
        assert_eq!(previa.total_a_pagar, Dinheiro::reais(150));
        assert_eq!(previa.amostra[2].pessoa, "Distribuidora Sul (novo)");
    }
}
