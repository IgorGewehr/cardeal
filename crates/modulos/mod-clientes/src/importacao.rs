//! Importação de clientes (e fornecedores) por planilha CSV — a implantação do primeiro dia:
//! trazer a carteira do sistema antigo sem digitar um por um.
//!
//! A primeira linha é o **cabeçalho**; as colunas valem pelo nome, em qualquer ordem (sem caixa
//! nem acento). Só `Nome` é obrigatória:
//! - `CPF/CNPJ` (ou `CPF`, `CNPJ`, `Documento`): o dígito verificador é conferido; CPF faz
//!   pessoa física, CNPJ jurídica. `Tipo` (PF/PJ) confere, quando vem, e vale sem documento.
//! - `Nome fantasia`, `Nascimento` (ou `Abertura`), `Limite de crédito`.
//! - `Celular`, `WhatsApp`, `Telefone`, `E-mail`: o formato é conferido.
//! - `Endereço` (ou `Logradouro`, `Rua`), `Número`, `Complemento`, `Bairro`, `Cidade`, `UF`,
//!   `CEP`: o endereço entra quando há logradouro, e então CEP, cidade e UF são exigidos.
//! - `Papel`: `Cliente` (o padrão) ou `Fornecedor`.
//!
//! [`planejar`] não grava nada: devolve quem criar, quantos já existiam e os erros por linha.
//! Já existe quem tem o mesmo CPF/CNPJ ou, sem documento, o mesmo nome — importar o mesmo
//! arquivo duas vezes não duplica. Quem aplica recusa o arquivo inteiro se houver um erro.

use std::collections::{HashMap, HashSet};

use cardeal_kernel::documento::Documento;
use cardeal_kernel::planilha::{chave_coluna, ler_linhas};
use cardeal_kernel::texto::chave_busca;
use cardeal_kernel::{Data, Dinheiro, Id};
use serde::{Deserialize, Serialize};

use crate::cadastro::{Contato, Endereco, TipoContato, TipoDocumento, TipoEndereco};
use crate::comandos::EnderecoInicial;
use crate::pessoa::{Papel, TipoPessoa};

/// Quantos clientes a prévia mostra.
pub const AMOSTRA_DA_PREVIA: usize = 200;

/// Uma coluna conhecida.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Coluna {
    Nome,
    Fantasia,
    Tipo,
    Documento,
    Celular,
    Whatsapp,
    Telefone,
    Email,
    Cep,
    Logradouro,
    Numero,
    Complemento,
    Bairro,
    Cidade,
    Uf,
    Nascimento,
    Limite,
    Papel,
}

fn coluna_de(titulo: &str) -> Option<Coluna> {
    Some(match chave_coluna(titulo).as_str() {
        "nome" | "nomecompleto" | "razaosocial" | "cliente" | "fornecedor" => Coluna::Nome,
        "nomefantasia" | "fantasia" => Coluna::Fantasia,
        "tipo" | "tipodepessoa" | "tipopessoa" | "pfpj" => Coluna::Tipo,
        "cpfcnpj" | "cnpjcpf" | "cpf" | "cnpj" | "documento" | "cpfoucnpj" => Coluna::Documento,
        "celular" | "telefonecelular" | "cel" => Coluna::Celular,
        "whatsapp" | "zap" => Coluna::Whatsapp,
        "telefone" | "fone" | "telefonefixo" => Coluna::Telefone,
        "email" => Coluna::Email,
        "cep" => Coluna::Cep,
        "endereco" | "logradouro" | "rua" => Coluna::Logradouro,
        "numero" | "nro" | "num" => Coluna::Numero,
        "complemento" => Coluna::Complemento,
        "bairro" => Coluna::Bairro,
        "cidade" | "municipio" => Coluna::Cidade,
        "uf" | "estado" => Coluna::Uf,
        "nascimento" | "datadenascimento" | "aniversario" | "abertura" | "datadeabertura" => {
            Coluna::Nascimento
        }
        "limite" | "limitedecredito" | "limitecredito" => Coluna::Limite,
        "papel" | "relacao" => Coluna::Papel,
        _ => return None,
    })
}

/// Quem já está no cadastro, para não duplicar: por CPF/CNPJ (só dígitos) e por nome
/// ([`chave_busca`]). Também serve à importação de contas em aberto do financeiro, que acha
/// a pessoa de cada conta por aqui.
#[derive(Debug, Clone, Default)]
pub struct IndicePessoas {
    /// CPF/CNPJ → pessoa.
    pub por_documento: HashMap<String, Id>,
    /// Nome normalizado → pessoas com esse nome.
    pub por_nome: HashMap<String, Vec<Id>>,
}

impl IndicePessoas {
    /// A pessoa de um CPF/CNPJ (com ou sem máscara).
    #[must_use]
    pub fn do_documento(&self, documento: &str) -> Option<Id> {
        self.por_documento
            .get(&cardeal_kernel::texto::somente_digitos(documento))
            .copied()
    }

    /// As pessoas com esse nome (sem caixa nem acento).
    #[must_use]
    pub fn do_nome(&self, nome: &str) -> &[Id] {
        self.por_nome
            .get(&chave_busca(nome))
            .map_or(&[], Vec::as_slice)
    }
}

/// Um cliente a criar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClienteNovo {
    /// A linha da planilha.
    pub linha: usize,
    /// Física ou jurídica.
    pub tipo: TipoPessoa,
    /// O nome.
    pub nome: String,
    /// O nome fantasia.
    pub nome_fantasia: Option<String>,
    /// O papel (cliente ou fornecedor).
    pub papel: Papel,
    /// O documento: o tipo e só os dígitos.
    pub documento: Option<(TipoDocumento, String)>,
    /// Nascimento ou abertura.
    pub nascimento: Option<Data>,
    /// Os contatos, na ordem das colunas (o primeiro é o principal).
    pub contatos: Vec<(TipoContato, String)>,
    /// O endereço.
    pub endereco: Option<EnderecoInicial>,
    /// O limite de crédito.
    pub limite: Option<Dinheiro>,
}

/// Um erro numa linha.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErroDeLinha {
    /// A linha no arquivo.
    pub linha: usize,
    /// O que está errado.
    pub mensagem: String,
}

/// O que a planilha faria.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanoClientes {
    /// Quem criar.
    pub novos: Vec<ClienteNovo>,
    /// Quantos já estavam no cadastro (ou se repetiam na planilha).
    pub ja_existiam: u32,
    /// Os erros por linha.
    pub erros: Vec<ErroDeLinha>,
    /// As colunas que a importação não conhece (ficam de fora, sem erro).
    pub colunas_ignoradas: Vec<String>,
}

/// Uma linha da prévia.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinhaPreviaCliente {
    /// O nome.
    pub nome: String,
    /// O documento formatado (ou vazio).
    pub documento: String,
    /// "Cliente" ou "Fornecedor".
    pub papel: String,
    /// O primeiro contato (ou vazio).
    pub contato: String,
    /// "Cidade/UF" (ou vazio).
    pub cidade: String,
    /// O limite de crédito.
    pub limite: Option<Dinheiro>,
}

/// O que a tela mostra antes de confirmar.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreviaImportacaoClientes {
    /// Quantos serão criados.
    pub novos: u32,
    /// Quantos já existiam.
    pub ja_existiam: u32,
    /// Quantos novos são fornecedores.
    pub fornecedores: u32,
    /// Quantos novos trazem limite de crédito.
    pub com_limite: u32,
    /// Os erros por linha.
    pub erros: Vec<ErroDeLinha>,
    /// As colunas que ficam de fora.
    pub colunas_ignoradas: Vec<String>,
    /// Os primeiros a criar (até [`AMOSTRA_DA_PREVIA`]).
    pub amostra: Vec<LinhaPreviaCliente>,
}

fn contar(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

impl PlanoClientes {
    /// O resumo para a tela.
    #[must_use]
    pub fn previa(&self) -> PreviaImportacaoClientes {
        PreviaImportacaoClientes {
            novos: contar(self.novos.len()),
            ja_existiam: self.ja_existiam,
            fornecedores: contar(
                self.novos
                    .iter()
                    .filter(|c| c.papel == Papel::Fornecedor)
                    .count(),
            ),
            com_limite: contar(self.novos.iter().filter(|c| c.limite.is_some()).count()),
            erros: self.erros.clone(),
            colunas_ignoradas: self.colunas_ignoradas.clone(),
            amostra: self
                .novos
                .iter()
                .take(AMOSTRA_DA_PREVIA)
                .map(|c| LinhaPreviaCliente {
                    nome: c.nome.clone(),
                    documento: c
                        .documento
                        .as_ref()
                        .and_then(|(_, d)| Documento::novo(d).ok())
                        .map_or_else(String::new, |d| d.to_string()),
                    papel: match c.papel {
                        Papel::Fornecedor => "Fornecedor",
                        _ => "Cliente",
                    }
                    .to_owned(),
                    contato: c
                        .contatos
                        .first()
                        .map_or_else(String::new, |(_, v)| v.clone()),
                    cidade: c
                        .endereco
                        .as_ref()
                        .map_or_else(String::new, |e| format!("{}/{}", e.cidade, e.uf)),
                    limite: c.limite,
                })
                .collect(),
        }
    }

    /// Verdadeiro se algum novo traz limite de crédito — que fora da importação exige a
    /// permissão de definir limite.
    #[must_use]
    pub fn define_limite(&self) -> bool {
        self.novos.iter().any(|c| c.limite.is_some())
    }
}

/// Uma célula preenchida.
fn celula(cels: &[String], cab: &HashMap<Coluna, usize>, c: Coluna) -> Option<String> {
    cab.get(&c)
        .and_then(|&i| cels.get(i))
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
}

fn ler_tipo(texto: &str) -> Result<TipoPessoa, String> {
    match chave_coluna(texto).as_str() {
        "pf" | "f" | "fisica" | "pessoafisica" => Ok(TipoPessoa::Fisica),
        "pj" | "j" | "juridica" | "pessoajuridica" | "empresa" => Ok(TipoPessoa::Juridica),
        _ => Err(format!("Tipo \"{texto}\": use PF ou PJ")),
    }
}

fn ler_papel(texto: &str) -> Result<Papel, String> {
    let p = chave_coluna(texto);
    if p.starts_with("cliente") {
        Ok(Papel::Cliente)
    } else if p.starts_with("fornecedor") {
        Ok(Papel::Fornecedor)
    } else {
        Err(format!("Papel \"{texto}\": use Cliente ou Fornecedor"))
    }
}

/// O endereço da linha, se houver logradouro.
fn ler_endereco(
    cels: &[String],
    cab: &HashMap<Coluna, usize>,
) -> Result<Option<EnderecoInicial>, String> {
    let get = |c| celula(cels, cab, c);
    let Some(logradouro) = get(Coluna::Logradouro) else {
        return Ok(None);
    };
    let cep = get(Coluna::Cep).ok_or("o endereço precisa do CEP")?;
    let cidade = get(Coluna::Cidade).ok_or("o endereço precisa da cidade")?;
    let uf = get(Coluna::Uf).ok_or("o endereço precisa da UF")?;
    let numero = get(Coluna::Numero).unwrap_or_else(|| "S/N".to_owned());
    let bairro = get(Coluna::Bairro).unwrap_or_default();
    // As mesmas regras do cadastro (UF válida, CEP com 8 dígitos).
    let end = Endereco::novo(
        Id::NULO,
        TipoEndereco::Cobranca,
        logradouro.clone(),
        numero.clone(),
        bairro.clone(),
        cidade.clone(),
        &uf,
        &cep,
    )
    .map_err(|e| format!("endereço: {e}"))?;
    Ok(Some(EnderecoInicial {
        tipo: TipoEndereco::Cobranca,
        logradouro,
        numero,
        complemento: get(Coluna::Complemento),
        bairro,
        cidade,
        uf: end.uf.sigla().to_owned(),
        cep: end.cep,
    }))
}

/// Lê uma linha; `Err` traz a mensagem.
fn ler_cliente(
    linha: usize,
    cels: &[String],
    cab: &HashMap<Coluna, usize>,
) -> Result<ClienteNovo, String> {
    let get = |c| celula(cels, cab, c);
    let nome = get(Coluna::Nome).ok_or("falta o nome")?;
    let documento = get(Coluna::Documento)
        .map(|bruto| {
            Documento::novo(&bruto)
                .map(|d| {
                    let tipo = if d.e_fisica() {
                        TipoDocumento::Cpf
                    } else {
                        TipoDocumento::Cnpj
                    };
                    (tipo, d.sem_mascara())
                })
                .map_err(|_| format!("CPF/CNPJ inválido: {bruto}"))
        })
        .transpose()?;
    let tipo_do_documento = documento.as_ref().map(|(t, _)| match t {
        TipoDocumento::Cnpj => TipoPessoa::Juridica,
        _ => TipoPessoa::Fisica,
    });
    let tipo_informado = get(Coluna::Tipo).map(|t| ler_tipo(&t)).transpose()?;
    let tipo = match (tipo_do_documento, tipo_informado) {
        (Some(d), Some(t)) if d != t => {
            return Err("o Tipo não confere com o documento (CPF é PF, CNPJ é PJ)".to_owned())
        }
        (Some(t), _) | (None, Some(t)) => t,
        (None, None) => TipoPessoa::Fisica,
    };
    let papel = get(Coluna::Papel)
        .map(|p| ler_papel(&p))
        .transpose()?
        .unwrap_or(Papel::Cliente);
    let nascimento = get(Coluna::Nascimento)
        .map(|d| d.parse::<Data>().map_err(|_| format!("data inválida: {d}")))
        .transpose()?;
    let limite = get(Coluna::Limite)
        .map(|v| {
            Dinheiro::de_str(&v)
                .ok()
                .filter(|d| !d.e_negativo())
                .ok_or(format!("limite de crédito inválido: {v}"))
        })
        .transpose()?;
    let mut contatos = Vec::new();
    for (coluna, tipo_contato) in [
        (Coluna::Celular, TipoContato::Celular),
        (Coluna::Whatsapp, TipoContato::Whatsapp),
        (Coluna::Telefone, TipoContato::Telefone),
        (Coluna::Email, TipoContato::Email),
    ] {
        if let Some(valor) = get(coluna) {
            Contato::novo(Id::NULO, tipo_contato, &valor).map_err(|e| format!("{e}: {valor}"))?;
            contatos.push((tipo_contato, valor));
        }
    }
    Ok(ClienteNovo {
        linha,
        tipo,
        nome,
        nome_fantasia: get(Coluna::Fantasia).filter(|_| tipo == TipoPessoa::Juridica),
        papel,
        documento,
        nascimento,
        contatos,
        endereco: ler_endereco(cels, cab)?,
        limite,
    })
}

/// Compara a planilha com o cadastro, sem gravar nada.
#[must_use]
pub fn planejar(texto: &str, existentes: &IndicePessoas) -> PlanoClientes {
    let mut plano = PlanoClientes::default();
    let mut linhas = ler_linhas(texto).into_iter();
    let Some((linha_cab, titulos)) = linhas.next() else {
        return plano; // arquivo vazio: nada a fazer
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
    if !cab.contains_key(&Coluna::Nome) {
        plano.erros.push(ErroDeLinha {
            linha: linha_cab,
            mensagem: "falta a coluna Nome no cabeçalho".to_owned(),
        });
        return plano;
    }
    // O que já está no cadastro mais o que a planilha já trouxe.
    let mut documentos: HashSet<String> = existentes.por_documento.keys().cloned().collect();
    let mut nomes: HashSet<String> = existentes.por_nome.keys().cloned().collect();
    for (linha, cels) in linhas {
        match ler_cliente(linha, &cels, &cab) {
            Err(mensagem) => plano.erros.push(ErroDeLinha { linha, mensagem }),
            Ok(c) => {
                let nome = chave_busca(&c.nome);
                let ja_existe = match &c.documento {
                    Some((_, d)) => !documentos.insert(d.clone()),
                    None => nomes.contains(&nome),
                };
                if ja_existe {
                    plano.ja_existiam += 1;
                } else {
                    nomes.insert(nome);
                    plano.novos.push(c);
                }
            }
        }
    }
    plano
}

#[cfg(test)]
mod testes {
    use super::*;

    fn indice(documentos: &[&str], nomes: &[&str]) -> IndicePessoas {
        IndicePessoas {
            por_documento: documentos
                .iter()
                .map(|d| ((*d).to_owned(), Id::novo()))
                .collect(),
            por_nome: nomes
                .iter()
                .map(|n| (chave_busca(n), vec![Id::novo()]))
                .collect(),
        }
    }

    #[test]
    fn colunas_em_qualquer_ordem_e_o_tipo_vem_do_documento() {
        let csv = "E-mail;CPF/CNPJ;Nome;Celular;Limite de crédito;Nome fantasia\n\
                   ana@exemplo.com;529.982.247-25;Ana Lima;(51) 99999-0000;500,00;Aninha\n\
                   ;11.222.333/0001-81;Loja do Zé Ltda;;;Loja do Zé\n";
        let p = planejar(csv, &IndicePessoas::default());
        assert!(p.erros.is_empty(), "{:?}", p.erros);
        assert_eq!(p.novos.len(), 2);
        let ana = &p.novos[0];
        assert_eq!(ana.tipo, TipoPessoa::Fisica);
        assert_eq!(
            ana.documento,
            Some((TipoDocumento::Cpf, "52998224725".into()))
        );
        assert_eq!(
            ana.contatos[0].0,
            TipoContato::Celular,
            "o celular vem antes"
        );
        assert_eq!(ana.contatos.len(), 2);
        assert_eq!(ana.limite, Some(Dinheiro::reais(500)));
        assert_eq!(ana.nome_fantasia, None, "fantasia só para PJ");
        let loja = &p.novos[1];
        assert_eq!(loja.tipo, TipoPessoa::Juridica);
        assert_eq!(loja.nome_fantasia.as_deref(), Some("Loja do Zé"));
        let previa = p.previa();
        assert_eq!(previa.amostra[1].documento, "11.222.333/0001-81");
        assert_eq!(previa.com_limite, 1);
    }

    #[test]
    fn erros_por_linha_e_quem_ja_existe_nao_duplica() {
        let csv = "Nome;CPF;Telefone;Endereço;Cidade;UF;CEP;Tipo\n\
                   Ana;529.982.247-25;;;;;;\n\
                   JOSÉ SILVA;;;;;;;\n\
                   Bruno;111.111.111-11;;;;;;\n\
                   Carla;;123;;;;;\n\
                   Dani;;;Rua A;Porto Alegre;XX;90000-000;\n\
                   Edu;;;Rua B;;RS;90000-000;\n\
                   Fábio;390.533.447-05;;;;;;PJ\n\
                   Gil;;;;;;;\n\
                   gil;;;;;;;\n";
        let p = planejar(csv, &indice(&["52998224725"], &["Jose Silva"]));
        assert_eq!(p.ja_existiam, 3, "Ana (CPF), José (nome) e o Gil repetido");
        assert_eq!(p.novos.len(), 1);
        let linhas: Vec<usize> = p.erros.iter().map(|e| e.linha).collect();
        assert_eq!(linhas, vec![4, 5, 6, 7, 8], "{:?}", p.erros);
    }

    #[test]
    fn endereco_completo_e_papel_fornecedor() {
        let csv = "Nome;Papel;Rua;Bairro;Cidade;UF;CEP\n\
                   Distribuidora Sul;Fornecedor;Av. Brasil;Centro;Canoas;rs;92010-000\n";
        let p = planejar(csv, &IndicePessoas::default());
        assert!(p.erros.is_empty(), "{:?}", p.erros);
        let d = &p.novos[0];
        assert_eq!(d.papel, Papel::Fornecedor);
        let e = d.endereco.as_ref().unwrap();
        assert_eq!(
            (e.numero.as_str(), e.uf.as_str(), e.cep.as_str()),
            ("S/N", "RS", "92010000")
        );
        assert_eq!(p.previa().fornecedores, 1);
    }

    #[test]
    fn sem_a_coluna_nome_nada_e_lido() {
        let p = planejar("CPF;Telefone\n52998224725;\n", &IndicePessoas::default());
        assert_eq!(p.erros.len(), 1);
        assert!(p.novos.is_empty());
    }
}
