//! Produto, variação de grade, código de barras, unidade e lote.
//!
//! `docs/modulos/estoque.md` §3, §4 e §11.7. Domínio puro — validação de NCM e de GTIN
//! (dígito verificador mod-10) sem I/O.

use cardeal_kernel::{texto, Data, Id, Quantidade, Versao};
use serde::{Deserialize, Serialize};

use crate::erros::ErroEstoque;

/// Um produto do catálogo.
// Os quatro `bool` (`controla_*`, `ativo`) espelham colunas do `docs/modulos/estoque.md`
// §13 — são flags de domínio independentes, não um enum de estado disfarçado.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Produto {
    /// Identidade.
    pub id: Id,
    /// A empresa.
    pub empresa: Id,
    /// O grupo (hierárquico, com perfil tributário).
    pub grupo_produto: Id,
    /// O nome.
    pub nome: String,
    /// NCM, 8 dígitos.
    pub ncm: String,
    /// CEST (quando o NCM exige ST).
    pub cest: Option<String>,
    /// Código de barras (GTIN — EAN-8/12/13/14), validado por dígito verificador. `None`
    /// para produtos sem código impresso (peça avulsa, serviço).
    pub codigo_barras: Option<String>,
    /// Fabricante da peça (ex.: "Texas Instruments", "Samsung SDI") — quem fabrica, não quem
    /// vende. Cadastro técnico de microeletrônica (§3): todos os campos abaixo são
    /// opcionais e "state of art" para peça de conserto (IC, capacitor, conector, tela,
    /// bateria, fonte), não para o catálogo genérico.
    pub fabricante: Option<String>,
    /// Código/part number do fabricante (MPN) — o identificador que bate com o datasheet;
    /// diferente do [`Produto::codigo_barras`] (que é do distribuidor/embalagem, não da peça
    /// em si).
    pub codigo_fabricante: Option<String>,
    /// Categoria técnica livre (ex.: "IC", "capacitor", "conector", "tela", "bateria",
    /// "fonte") — uma etiqueta rápida de bancada para filtrar por tipo de componente;
    /// **não** substitui [`Produto::grupo_produto`] (que é hierárquico e carrega o perfil
    /// tributário).
    pub categoria_tecnica: Option<String>,
    /// Resumo técnico/datasheet em texto livre (tensão, corrente, pinagem, o que for
    /// relevante para o técnico de bancada). Sem parser de datasheet — só o que foi anotado.
    pub especificacao_tecnica: Option<String>,
    /// Em quais aparelhos/modelos esta peça serve, texto livre (ex.: "iPhone 11 / 11 Pro /
    /// XR").
    pub compatibilidade: Option<String>,
    /// Garantia do fornecedor sobre esta peça, em dias — **diferente** da garantia que a
    /// assistência dá ao cliente sobre o serviço (`OrdemServico::garantia_dias`, em
    /// `mod-os`).
    pub garantia_fornecedor_dias: Option<u16>,
    /// Localização física no estoque (prateleira/gaveta) — ajuda muito no balcão na hora de
    /// achar a peça rápido.
    pub localizacao_fisica: Option<String>,
    /// Se o saldo vive em [`Variacao`] (grade cor/tamanho).
    pub controla_grade: bool,
    /// Se o produto é rastreado por lote.
    pub controla_lote: bool,
    /// Se o produto tem validade (exige `controla_lote`).
    pub controla_validade: bool,
    /// A unidade padrão.
    pub unidade_padrao: Id,
    /// Ponto de pedido — dispara alerta quando o disponível cruza abaixo.
    pub ponto_pedido: Option<Quantidade>,
    /// Estoque mínimo (curva ABC, sugestão de compra).
    pub estoque_minimo: Option<Quantidade>,
    /// Estoque máximo.
    pub estoque_maximo: Option<Quantidade>,
    /// Se está ativo.
    pub ativo: bool,
    /// Versão para bloqueio otimista.
    pub versao: Versao,
}

impl Produto {
    /// Cria um produto validando NCM e a coerência lote/validade.
    ///
    /// # Errors
    /// [`ErroEstoque::NomeVazio`], [`ErroEstoque::NcmInvalido`], [`ErroEstoque::ValidadeSemLote`].
    pub fn novo(
        empresa: Id,
        grupo_produto: Id,
        nome: impl Into<String>,
        ncm: &str,
        unidade_padrao: Id,
    ) -> Result<Self, ErroEstoque> {
        let nome = nome.into().trim().to_string();
        if nome.is_empty() {
            return Err(ErroEstoque::NomeVazio);
        }
        let ncm = texto::somente_digitos(ncm);
        if ncm.len() != 8 {
            return Err(ErroEstoque::NcmInvalido);
        }
        Ok(Self {
            id: Id::novo(),
            empresa,
            grupo_produto,
            nome,
            ncm,
            cest: None,
            codigo_barras: None,
            fabricante: None,
            codigo_fabricante: None,
            categoria_tecnica: None,
            especificacao_tecnica: None,
            compatibilidade: None,
            garantia_fornecedor_dias: None,
            localizacao_fisica: None,
            controla_grade: false,
            controla_lote: false,
            controla_validade: false,
            unidade_padrao,
            ponto_pedido: None,
            estoque_minimo: None,
            estoque_maximo: None,
            ativo: true,
            versao: Versao::INICIAL,
        })
    }

    /// Liga os controles de rastreabilidade, recusando validade sem lote.
    ///
    /// # Errors
    /// [`ErroEstoque::ValidadeSemLote`].
    pub fn com_rastreabilidade(
        mut self,
        controla_lote: bool,
        controla_validade: bool,
    ) -> Result<Self, ErroEstoque> {
        if controla_validade && !controla_lote {
            return Err(ErroEstoque::ValidadeSemLote);
        }
        self.controla_lote = controla_lote;
        self.controla_validade = controla_validade;
        Ok(self)
    }

    /// Verdadeiro se o disponível informado está abaixo do ponto de pedido.
    #[must_use]
    pub fn abaixo_do_ponto(&self, disponivel: Quantidade) -> bool {
        self.ponto_pedido.is_some_and(|pp| disponivel < pp)
    }

    /// Define o código de barras, validando o dígito verificador.
    ///
    /// # Errors
    /// [`ErroEstoque::GtinComprimento`], [`ErroEstoque::GtinInvalido`].
    pub fn com_codigo_barras(mut self, gtin: &str) -> Result<Self, ErroEstoque> {
        self.codigo_barras = Some(validar_gtin(gtin)?);
        Ok(self)
    }

    /// Define os detalhes técnicos (todos opcionais) — string vazia normaliza para `None`,
    /// para o formulário poder enviar `""` sem sujar o banco com string vazia em vez de
    /// `NULL`.
    #[must_use]
    pub fn com_detalhes_tecnicos(mut self, detalhes: DetalhesTecnicos) -> Self {
        self.fabricante = normalizar_opcional(detalhes.fabricante);
        self.codigo_fabricante = normalizar_opcional(detalhes.codigo_fabricante);
        self.categoria_tecnica = normalizar_opcional(detalhes.categoria_tecnica);
        self.especificacao_tecnica = normalizar_opcional(detalhes.especificacao_tecnica);
        self.compatibilidade = normalizar_opcional(detalhes.compatibilidade);
        self.garantia_fornecedor_dias = detalhes.garantia_fornecedor_dias;
        self.localizacao_fisica = normalizar_opcional(detalhes.localizacao_fisica);
        self
    }
}

/// `Some("  ")`/`Some("")` viram `None`; qualquer outro `Some` vem trimado.
fn normalizar_opcional(s: Option<String>) -> Option<String> {
    s.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

/// Detalhes técnicos opcionais de um produto — o cadastro para peça de microeletrônica
/// (`docs/modulos/estoque.md` §3): fabricante/MPN para casar com o datasheet, categoria
/// técnica e especificação em texto livre, compatibilidade com aparelhos, garantia do
/// fornecedor sobre a peça e localização física no estoque. Nenhum campo é obrigatório —
/// nome e preço continuam sendo o essencial do cadastro rápido.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DetalhesTecnicos {
    /// Fabricante da peça.
    pub fabricante: Option<String>,
    /// Código/part number do fabricante (MPN).
    pub codigo_fabricante: Option<String>,
    /// Categoria técnica livre (ex.: "IC", "capacitor", "tela").
    pub categoria_tecnica: Option<String>,
    /// Especificação/resumo de datasheet em texto livre.
    pub especificacao_tecnica: Option<String>,
    /// Compatibilidade/aplicação em texto livre.
    pub compatibilidade: Option<String>,
    /// Garantia do fornecedor sobre a peça, em dias.
    pub garantia_fornecedor_dias: Option<u16>,
    /// Localização física (prateleira/gaveta).
    pub localizacao_fisica: Option<String>,
}

/// Uma variação de grade (cor/tamanho) — só existe se `produto.controla_grade`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Variacao {
    /// Identidade.
    pub id: Id,
    /// O produto dono.
    pub produto: Id,
    /// Cor.
    pub cor: Option<String>,
    /// Tamanho.
    pub tamanho: Option<String>,
    /// SKU, único dentro do produto.
    pub sku: String,
    /// Se está ativa.
    pub ativo: bool,
}

impl Variacao {
    /// Cria uma variação, recusando se o produto não controla grade.
    ///
    /// # Errors
    /// [`ErroEstoque::ProdutoSemGrade`].
    pub fn nova(produto: &Produto, sku: impl Into<String>) -> Result<Self, ErroEstoque> {
        if !produto.controla_grade {
            return Err(ErroEstoque::ProdutoSemGrade);
        }
        Ok(Self {
            id: Id::novo(),
            produto: produto.id,
            cor: None,
            tamanho: None,
            sku: sku.into(),
            ativo: true,
        })
    }
}

/// Valida um GTIN (EAN-8/12/13/14) pelo dígito verificador mod-10.
///
/// # Errors
/// [`ErroEstoque::GtinComprimento`] se não tiver 8/12/13/14 dígitos;
/// [`ErroEstoque::GtinInvalido`] se o dígito verificador não conferir.
pub fn validar_gtin(gtin: &str) -> Result<String, ErroEstoque> {
    let d = texto::somente_digitos(gtin);
    if !matches!(d.len(), 8 | 12 | 13 | 14) {
        return Err(ErroEstoque::GtinComprimento);
    }
    let bytes = d.as_bytes();
    let n = bytes.len();
    let mut soma = 0u32;
    // Da direita para a esquerda, ignorando o último dígito (o verificador): pesos 3,1,3,1…
    for (i, b) in bytes[..n - 1].iter().rev().enumerate() {
        let valor = u32::from(b - b'0');
        soma += if i % 2 == 0 { valor * 3 } else { valor };
    }
    let verificador = (10 - (soma % 10)) % 10;
    if verificador == u32::from(bytes[n - 1] - b'0') {
        Ok(d)
    } else {
        Err(ErroEstoque::GtinInvalido)
    }
}

/// Uma unidade de medida da empresa.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Unidade {
    /// Identidade.
    pub id: Id,
    /// A empresa.
    pub empresa: Id,
    /// Sigla: "UN", "KG", "CX".
    pub sigla: String,
    /// Nome por extenso.
    pub nome: String,
    /// Se aceita fração.
    pub fracionavel: bool,
}

/// Conversão entre duas unidades para um produto: `1 unidade_origem = fator × unidade_destino`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Conversao {
    /// O produto.
    pub produto: Id,
    /// A unidade de origem.
    pub unidade_origem: Id,
    /// A unidade de destino.
    pub unidade_destino: Id,
    /// O fator (ex.: 1 CX = 12 UN → `fator = 12`).
    pub fator: Quantidade,
}

impl Conversao {
    /// Converte uma quantidade da unidade de origem para a de destino.
    #[must_use]
    pub fn aplicar(&self, quantidade: Quantidade) -> Quantidade {
        // qtd (1e-4) × fator (1e-4) / 1e-4 = 1e-4
        let bruto =
            i128::from(quantidade.unidades_internas()) * i128::from(self.fator.unidades_internas());
        Quantidade::interna(i64::try_from(bruto / 10_000).unwrap_or(i64::MAX))
    }
}

/// O estado de um [`Lote`] (`docs/modulos/estoque.md` §4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EstadoLote {
    /// Ainda tem saldo e não venceu.
    Ativo,
    /// A validade já passou.
    Vencido,
    /// O saldo chegou a zero.
    Esgotado,
}

/// Um lote de um produto rastreado.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lote {
    /// Identidade.
    pub id: Id,
    /// A empresa.
    pub empresa: Id,
    /// O produto.
    pub produto: Id,
    /// Número do lote.
    pub numero_lote: String,
    /// Data de fabricação.
    pub fabricacao: Option<Data>,
    /// Data de validade (obrigatória se `produto.controla_validade`).
    pub validade: Option<Data>,
    /// Fornecedor de origem.
    pub fornecedor: Option<Id>,
    /// Quantidade da entrada que criou o lote.
    pub quantidade_inicial: Quantidade,
    /// O estado atual.
    pub estado: EstadoLote,
}

impl Lote {
    /// Reavalia o estado do lote conforme o saldo e a data (verificação diária).
    pub fn reavaliar(&mut self, saldo: Quantidade, hoje: Data) {
        self.estado = if saldo.unidades_internas() <= 0 {
            EstadoLote::Esgotado
        } else if self.validade.is_some_and(|v| v < hoje) {
            EstadoLote::Vencido
        } else {
            EstadoLote::Ativo
        };
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn produto() -> Produto {
        Produto::novo(
            Id::novo(),
            Id::novo(),
            "Refrigerante Cola 2L",
            "2202.10.00",
            Id::novo(),
        )
        .unwrap()
    }

    #[test]
    fn ncm_precisa_de_oito_digitos() {
        assert_eq!(produto().ncm, "22021000");
        assert_eq!(
            Produto::novo(Id::novo(), Id::novo(), "X", "123", Id::novo()).unwrap_err(),
            ErroEstoque::NcmInvalido
        );
    }

    #[test]
    fn validade_exige_lote() {
        assert_eq!(
            produto().com_rastreabilidade(false, true).unwrap_err(),
            ErroEstoque::ValidadeSemLote
        );
        assert!(produto().com_rastreabilidade(true, true).is_ok());
    }

    #[test]
    fn codigo_de_barras_valida_e_normaliza() {
        let p = produto().com_codigo_barras("789 4900 011517").unwrap();
        assert_eq!(p.codigo_barras.as_deref(), Some("7894900011517"));
        assert_eq!(
            produto().com_codigo_barras("7894900011518").unwrap_err(),
            ErroEstoque::GtinInvalido
        );
    }

    #[test]
    fn detalhes_tecnicos_normaliza_vazio_para_none() {
        let p = produto().com_detalhes_tecnicos(DetalhesTecnicos {
            fabricante: Some("Texas Instruments".to_string()),
            codigo_fabricante: Some("  ".to_string()),
            categoria_tecnica: Some("IC".to_string()),
            especificacao_tecnica: Some(String::new()),
            compatibilidade: Some("iPhone 11 / 11 Pro".to_string()),
            garantia_fornecedor_dias: Some(90),
            localizacao_fisica: None,
        });
        assert_eq!(p.fabricante.as_deref(), Some("Texas Instruments"));
        assert_eq!(p.codigo_fabricante, None); // só espaços -> None
        assert_eq!(p.categoria_tecnica.as_deref(), Some("IC"));
        assert_eq!(p.especificacao_tecnica, None); // vazio -> None
        assert_eq!(p.compatibilidade.as_deref(), Some("iPhone 11 / 11 Pro"));
        assert_eq!(p.garantia_fornecedor_dias, Some(90));
        assert_eq!(p.localizacao_fisica, None);
    }

    #[test]
    fn variacao_so_com_grade() {
        let p = produto();
        assert_eq!(
            Variacao::nova(&p, "SKU-1").unwrap_err(),
            ErroEstoque::ProdutoSemGrade
        );
        let mut p2 = produto();
        p2.controla_grade = true;
        assert!(Variacao::nova(&p2, "SKU-1").is_ok());
    }

    #[test]
    fn gtin_valida_digito_verificador() {
        // EAN-13 real (Coca-Cola), com e sem máscara.
        assert_eq!(validar_gtin("789 4900 011517").unwrap(), "7894900011517");
        assert_eq!(
            validar_gtin("7894900011518").unwrap_err(),
            ErroEstoque::GtinInvalido
        );
        assert_eq!(
            validar_gtin("12345").unwrap_err(),
            ErroEstoque::GtinComprimento
        );
        // EAN-8 válido.
        assert!(validar_gtin("40170725").is_ok());
    }

    #[test]
    fn conversao_de_caixa_para_unidade() {
        let c = Conversao {
            produto: Id::novo(),
            unidade_origem: Id::novo(),
            unidade_destino: Id::novo(),
            fator: Quantidade::unidades(12),
        };
        assert_eq!(c.aplicar(Quantidade::unidades(3)), Quantidade::unidades(36));
    }

    #[test]
    fn lote_reavalia_estado() {
        use cardeal_kernel::Fuso;
        let hoje = Data::hoje(Fuso::BRASILIA);
        let mut lote = Lote {
            id: Id::novo(),
            empresa: Id::novo(),
            produto: Id::novo(),
            numero_lote: "2408A".into(),
            fabricacao: None,
            validade: Some(hoje.mais_dias(-1)),
            fornecedor: None,
            quantidade_inicial: Quantidade::unidades(100),
            estado: EstadoLote::Ativo,
        };
        lote.reavaliar(Quantidade::unidades(10), hoje);
        assert_eq!(lote.estado, EstadoLote::Vencido);
        lote.reavaliar(Quantidade::ZERO, hoje);
        assert_eq!(lote.estado, EstadoLote::Esgotado);
        lote.validade = Some(hoje.mais_dias(30));
        lote.reavaliar(Quantidade::unidades(5), hoje);
        assert_eq!(lote.estado, EstadoLote::Ativo);
    }
}
