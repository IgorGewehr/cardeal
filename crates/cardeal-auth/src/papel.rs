//! Papéis — conjuntos nomeados de permissões e limites atribuídos a um usuário.
//!
//! Ver `docs/08-seguranca-permissoes.md` §3.2. Um papel é uma coleção de chaves de
//! permissão (as strings declaradas pelos manifestos dos módulos) mais os limites
//! quantitativos de §3.4. Papéis de fábrica ([`PapelDeFabrica`]) são imutáveis: o admin
//! cria cópias editáveis a partir deles.
//!
//! Este módulo **não** conhece o catálogo de permissões — ele vive em `cardeal-modkit` e
//! `cardeal-modkit` depende (adiante) deste crate para o despacho. Para não fechar um ciclo,
//! a expansão "papel de fábrica → permissões concretas" recebe o catálogo por parâmetro
//! ([`PoliticaPapel::aplicar`]).

use std::collections::{BTreeMap, BTreeSet};

use cardeal_kernel::{Id, Versao};

use crate::erros::ErroAuth;
use crate::limite::ValorLimite;

/// Um papel atribuível: nome, permissões e limites. `empresa` é `None` para papéis globais
/// do sistema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Papel {
    /// Identidade do papel.
    pub id: Id,
    /// A empresa dona; `None` para papéis de fábrica globais.
    pub empresa: Option<Id>,
    /// Nome exibido no cadastro ("Gerente", "Caixa da frente").
    pub nome: String,
    /// Descrição livre.
    pub descricao: String,
    /// Papel de fábrica: imutável (`sistema = 1` no banco).
    pub sistema: bool,
    /// As chaves de permissão concedidas.
    pub permissoes: BTreeSet<String>,
    /// Os limites quantitativos, por chave de limite.
    pub limites: BTreeMap<String, ValorLimite>,
    /// Versão para bloqueio otimista.
    pub versao: Versao,
}

impl Papel {
    /// Cria um papel vazio e editável para uma empresa.
    #[must_use]
    pub fn novo(empresa: Id, nome: impl Into<String>) -> Self {
        Self {
            id: Id::novo(),
            empresa: Some(empresa),
            nome: nome.into(),
            descricao: String::new(),
            sistema: false,
            permissoes: BTreeSet::new(),
            limites: BTreeMap::new(),
            versao: Versao::INICIAL,
        }
    }

    /// Concede uma permissão (encadeável).
    #[must_use]
    pub fn com_permissao(mut self, chave: impl Into<String>) -> Self {
        self.permissoes.insert(chave.into());
        self
    }

    /// Define um limite quantitativo (encadeável).
    #[must_use]
    pub fn com_limite(mut self, chave: impl Into<String>, valor: ValorLimite) -> Self {
        self.limites.insert(chave.into(), valor);
        self
    }

    /// Verdadeiro se o papel concede exatamente esta chave de permissão.
    #[must_use]
    pub fn concede(&self, permissao: &str) -> bool {
        self.permissoes.contains(permissao)
    }

    /// Garante que o papel pode ser alterado pelo admin.
    ///
    /// # Errors
    /// [`ErroAuth::PapelDoSistema`] se for um papel de fábrica.
    pub fn garantir_editavel(&self) -> Result<(), ErroAuth> {
        if self.sistema {
            return Err(ErroAuth::PapelDoSistema);
        }
        Ok(())
    }

    /// Deriva uma cópia editável deste papel para a empresa — o caminho suportado para
    /// "ajustar um papel de fábrica" (`docs/08 §3.2`).
    #[must_use]
    pub fn duplicar_para(&self, empresa: Id, nome: impl Into<String>) -> Self {
        Self {
            id: Id::novo(),
            empresa: Some(empresa),
            nome: nome.into(),
            descricao: self.descricao.clone(),
            sistema: false,
            permissoes: self.permissoes.clone(),
            limites: self.limites.clone(),
            versao: Versao::INICIAL,
        }
    }
}

/// Os papéis de fábrica de `docs/08 §3.2`. Imutáveis; servem de base para cópias.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PapelDeFabrica {
    /// Dono / TI — tudo, inclusive reabrir período.
    Administrador,
    /// Gerente de loja — tudo operacional; sem plano de contas nem reabertura.
    Gerente,
    /// Responsável financeiro — financeiro completo; leitura em vendas e estoque.
    Financeiro,
    /// Frente de caixa — vender e operar o caixa; sem custo nem margem.
    OperadorCaixa,
    /// Balcão / externo — vender, orçar e ver os próprios clientes; sem financeiro.
    Vendedor,
    /// Depósito — entrada, inventário e transferência; sem preço de venda.
    Estoquista,
    /// Compras — cotação, pedido e entrada por XML.
    Comprador,
    /// Escritório contábil — somente leitura e exportações.
    Contador,
    /// Auditoria — somente leitura, inclusive auditoria e razão.
    Auditor,
}

impl PapelDeFabrica {
    /// Todos os papéis de fábrica, para semear na instalação.
    pub const TODOS: [Self; 9] = [
        Self::Administrador,
        Self::Gerente,
        Self::Financeiro,
        Self::OperadorCaixa,
        Self::Vendedor,
        Self::Estoquista,
        Self::Comprador,
        Self::Contador,
        Self::Auditor,
    ];

    /// O identificador estável do papel (`snake_case`), usado como chave de configuração.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::Administrador => "administrador",
            Self::Gerente => "gerente",
            Self::Financeiro => "financeiro",
            Self::OperadorCaixa => "operador_caixa",
            Self::Vendedor => "vendedor",
            Self::Estoquista => "estoquista",
            Self::Comprador => "comprador",
            Self::Contador => "contador",
            Self::Auditor => "auditor",
        }
    }

    /// O nome exibido.
    #[must_use]
    pub const fn nome(self) -> &'static str {
        match self {
            Self::Administrador => "Administrador",
            Self::Gerente => "Gerente",
            Self::Financeiro => "Financeiro",
            Self::OperadorCaixa => "Operador de Caixa",
            Self::Vendedor => "Vendedor",
            Self::Estoquista => "Estoquista",
            Self::Comprador => "Comprador",
            Self::Contador => "Contador",
            Self::Auditor => "Auditor",
        }
    }

    /// A descrição de §3.2.
    #[must_use]
    pub const fn descricao(self) -> &'static str {
        match self {
            Self::Administrador => "Dono ou TI. Acesso total, inclusive reabrir período fechado.",
            Self::Gerente => {
                "Gerente de loja. Tudo operacional; sem plano de contas nem reabertura."
            }
            Self::Financeiro => {
                "Responsável financeiro. Financeiro completo; leitura em vendas e estoque."
            }
            Self::OperadorCaixa => {
                "Frente de caixa. Vender e operar o caixa; sem ver custo nem margem."
            }
            Self::Vendedor => {
                "Balcão ou externo. Vender, orçar e ver os próprios clientes; sem financeiro."
            }
            Self::Estoquista => {
                "Depósito. Entrada, inventário e transferência; sem preço de venda."
            }
            Self::Comprador => "Compras. Cotação, pedido e entrada por XML.",
            Self::Contador => "Escritório contábil. Somente leitura e exportações.",
            Self::Auditor => "Auditoria. Somente leitura, inclusive auditoria e razão.",
        }
    }

    /// A política de concessão do papel — a regra que, aplicada ao catálogo real de
    /// permissões, produz o conjunto concreto. Conservadora por natureza: nega o que não é
    /// claramente do papel e trata leitura ampla por sufixo.
    #[must_use]
    pub const fn politica(self) -> PoliticaPapel {
        const LEITURA: &[&str] = &[".ver", ".listar", ".consultar"];
        match self {
            Self::Administrador => PoliticaPapel {
                tudo: true,
                nega_prefixo: &[],
                modulos: &[],
                concede_prefixo: &[],
                concede_sufixo: &[],
            },
            Self::Gerente => PoliticaPapel {
                tudo: true,
                nega_prefixo: &[
                    "financeiro.conta",
                    "razao.reabrir_periodo",
                    "razao.estornar",
                ],
                modulos: &[],
                concede_prefixo: &[],
                concede_sufixo: &[],
            },
            Self::Financeiro => PoliticaPapel {
                tudo: false,
                nega_prefixo: &["razao.reabrir_periodo"],
                modulos: &["financeiro", "razao"],
                concede_prefixo: &[],
                concede_sufixo: LEITURA,
            },
            Self::OperadorCaixa => PoliticaPapel {
                tudo: false,
                nega_prefixo: &["estoque.custo", "vendas.margem", "pdv.margem", "relatorio"],
                modulos: &["pdv"],
                concede_prefixo: &["vendas.venda", "financeiro.caixa"],
                concede_sufixo: &[],
            },
            Self::Vendedor => PoliticaPapel {
                tudo: false,
                nega_prefixo: &["financeiro", "estoque.custo", "vendas.margem"],
                modulos: &[],
                concede_prefixo: &[
                    "vendas.orcamento",
                    "vendas.pedido",
                    "vendas.venda",
                    "clientes",
                ],
                concede_sufixo: &[],
            },
            Self::Estoquista => PoliticaPapel {
                tudo: false,
                nega_prefixo: &["estoque.preco", "vendas", "financeiro"],
                modulos: &["estoque"],
                concede_prefixo: &[],
                concede_sufixo: &[],
            },
            Self::Comprador => PoliticaPapel {
                tudo: false,
                nega_prefixo: &[],
                modulos: &["compras"],
                concede_prefixo: &["estoque.produto", "estoque.entrada"],
                concede_sufixo: LEITURA,
            },
            Self::Contador => PoliticaPapel {
                tudo: false,
                nega_prefixo: &[],
                modulos: &[],
                concede_prefixo: &["relatorio"],
                concede_sufixo: &[".ver", ".listar", ".consultar", ".exportar"],
            },
            Self::Auditor => PoliticaPapel {
                tudo: false,
                nega_prefixo: &[],
                modulos: &[],
                concede_prefixo: &[
                    "auditoria.ver",
                    "auditoria.listar",
                    "razao.ver",
                    "razao.provar",
                ],
                concede_sufixo: LEITURA,
            },
        }
    }

    /// Constrói o [`Papel`] concreto (global, `sistema = true`) aplicando a [`politica`] ao
    /// catálogo informado — as chaves de permissão declaradas pelos módulos ativos.
    ///
    /// [`politica`]: Self::politica
    #[must_use]
    pub fn materializar<'a>(self, catalogo: impl IntoIterator<Item = &'a str>) -> Papel {
        Papel {
            id: Id::novo(),
            empresa: None,
            nome: self.nome().to_string(),
            descricao: self.descricao().to_string(),
            sistema: true,
            permissoes: self.politica().aplicar(catalogo),
            limites: BTreeMap::new(),
            versao: Versao::INICIAL,
        }
    }
}

/// A regra declarativa de um papel de fábrica. Aplicada a um catálogo de chaves, produz o
/// conjunto concreto de permissões. A ordem de decisão por chave é: nega vence; depois
/// `tudo`; depois módulo completo, prefixo ou sufixo concedido.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PoliticaPapel {
    /// Concede qualquer permissão (papel de administrador).
    pub tudo: bool,
    /// Prefixos negados — vencem qualquer concessão.
    pub nega_prefixo: &'static [&'static str],
    /// Módulos concedidos por completo (o primeiro segmento da chave).
    pub modulos: &'static [&'static str],
    /// Prefixos concedidos.
    pub concede_prefixo: &'static [&'static str],
    /// Sufixos concedidos em todo o catálogo (tipicamente `.ver`/`.listar` para leitura ampla).
    pub concede_sufixo: &'static [&'static str],
}

impl PoliticaPapel {
    /// Decide se esta política concede uma chave de permissão.
    #[must_use]
    pub fn concede(&self, permissao: &str) -> bool {
        if self.nega_prefixo.iter().any(|p| permissao.starts_with(p)) {
            return false;
        }
        if self.tudo {
            return true;
        }
        let modulo = permissao.split('.').next().unwrap_or(permissao);
        self.modulos.contains(&modulo)
            || self
                .concede_prefixo
                .iter()
                .any(|p| permissao.starts_with(p))
            || self.concede_sufixo.iter().any(|s| permissao.ends_with(s))
    }

    /// Filtra um catálogo de chaves, devolvendo só as concedidas.
    #[must_use]
    pub fn aplicar<'a>(&self, catalogo: impl IntoIterator<Item = &'a str>) -> BTreeSet<String> {
        catalogo
            .into_iter()
            .filter(|p| self.concede(p))
            .map(String::from)
            .collect()
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    /// Um catálogo sintético que cobre os casos das descrições de §3.2.
    const CATALOGO: &[&str] = &[
        "financeiro.pulso.ver",
        "financeiro.receber.baixar",
        "financeiro.conta.editar",
        "financeiro.caixa.abrir",
        "razao.lancamento.ver",
        "razao.estornar",
        "razao.reabrir_periodo",
        "razao.provar",
        "vendas.pedido.ver",
        "vendas.pedido.faturar",
        "vendas.venda.registrar",
        "vendas.margem.ver",
        "estoque.produto.ver",
        "estoque.entrada.registrar",
        "estoque.inventario.contar",
        "estoque.preco.editar",
        "estoque.custo.ver",
        "compras.pedido.criar",
        "clientes.pessoa.editar",
        "pdv.cupom.cancelar",
        "auditoria.ver",
        "relatorio.exportar",
    ];

    fn permissoes(papel: PapelDeFabrica) -> BTreeSet<String> {
        papel.politica().aplicar(CATALOGO.iter().copied())
    }

    #[test]
    fn administrador_concede_tudo() {
        assert_eq!(
            permissoes(PapelDeFabrica::Administrador).len(),
            CATALOGO.len()
        );
    }

    #[test]
    fn gerente_tem_tudo_menos_plano_de_contas_e_reabertura() {
        let p = permissoes(PapelDeFabrica::Gerente);
        assert!(p.contains("vendas.pedido.faturar"));
        assert!(p.contains("financeiro.receber.baixar"));
        assert!(!p.contains("financeiro.conta.editar"));
        assert!(!p.contains("razao.reabrir_periodo"));
        assert!(!p.contains("razao.estornar"));
    }

    #[test]
    fn financeiro_ve_vendas_mas_nao_fatura() {
        let p = permissoes(PapelDeFabrica::Financeiro);
        assert!(p.contains("financeiro.receber.baixar"));
        assert!(p.contains("razao.lancamento.ver"));
        assert!(p.contains("vendas.pedido.ver"));
        assert!(!p.contains("vendas.pedido.faturar"));
        assert!(!p.contains("razao.reabrir_periodo"));
    }

    #[test]
    fn operador_de_caixa_nao_ve_custo_nem_margem() {
        let p = permissoes(PapelDeFabrica::OperadorCaixa);
        assert!(p.contains("vendas.venda.registrar"));
        assert!(p.contains("financeiro.caixa.abrir"));
        assert!(!p.contains("estoque.custo.ver"));
        assert!(!p.contains("vendas.margem.ver"));
    }

    #[test]
    fn estoquista_nao_ve_preco_de_venda() {
        let p = permissoes(PapelDeFabrica::Estoquista);
        assert!(p.contains("estoque.entrada.registrar"));
        assert!(p.contains("estoque.inventario.contar"));
        assert!(!p.contains("estoque.preco.editar"));
        assert!(!p.contains("vendas.venda.registrar"));
    }

    #[test]
    fn contador_e_somente_leitura_com_exportacao() {
        let p = permissoes(PapelDeFabrica::Contador);
        assert!(p.contains("financeiro.pulso.ver"));
        assert!(p.contains("razao.lancamento.ver"));
        assert!(p.contains("relatorio.exportar"));
        assert!(!p.contains("financeiro.receber.baixar"));
        assert!(!p.contains("vendas.pedido.faturar"));
    }

    #[test]
    fn auditor_le_o_razao_e_a_auditoria_sem_escrever() {
        let p = permissoes(PapelDeFabrica::Auditor);
        assert!(p.contains("auditoria.ver"));
        assert!(p.contains("razao.lancamento.ver"));
        assert!(p.contains("razao.provar"));
        assert!(!p.contains("razao.estornar"));
        assert!(!p.contains("financeiro.receber.baixar"));
    }

    #[test]
    fn papel_de_fabrica_materializado_e_imutavel() {
        let papel = PapelDeFabrica::Vendedor.materializar(CATALOGO.iter().copied());
        assert!(papel.sistema);
        assert!(papel.garantir_editavel().is_err());
        assert_eq!(papel.empresa, None);
    }

    #[test]
    fn copia_de_papel_de_fabrica_e_editavel_e_da_empresa() {
        let empresa = Id::novo();
        let base = PapelDeFabrica::Vendedor.materializar(CATALOGO.iter().copied());
        let copia = base.duplicar_para(empresa, "Vendedor externo");
        assert!(!copia.sistema);
        assert_eq!(copia.empresa, Some(empresa));
        assert_eq!(copia.permissoes, base.permissoes);
        assert!(copia.garantir_editavel().is_ok());
    }

    #[test]
    fn papel_editavel_encadeia_permissoes_e_limites() {
        let empresa = Id::novo();
        let papel = Papel::novo(empresa, "Caixa")
            .com_permissao("vendas.venda.registrar")
            .com_limite(
                "vendas.desconto_maximo",
                ValorLimite::Percentual(cardeal_kernel::Percentual::pontos(5)),
            );
        assert!(papel.concede("vendas.venda.registrar"));
        assert!(!papel.concede("vendas.pedido.faturar"));
        assert_eq!(papel.limites.len(), 1);
    }
}
