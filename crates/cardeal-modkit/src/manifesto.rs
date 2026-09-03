//! O manifesto: a identidade e as capacidades de um módulo, declaradas de uma vez.
//!
//! Ver `docs/04-pilar-modularidade.md` §2. Um manifesto é dado estático (`&'static`) —
//! nasce com o binário, não com o banco. É o que o registro do motor lê no boot para montar
//! menu, permissões e grafo de dependências, e o que `xtask verificar-perfis`
//! (`docs/14-testes-qualidade.md` §7) usa para provar que nenhum módulo desligado deixa
//! resto na interface.

use std::collections::HashSet;
use std::fmt;

use cardeal_ledger::PapelConta;

use crate::icone::Icone;
use crate::permissao::Permissao;

/// Identificador estável de um módulo, ex.: `"financeiro"`, `"estoque"`.
///
/// Newtype sobre `&'static str` — não `String` — porque todo `IdModulo` do sistema nasce de
/// uma constante no código-fonte de algum crate `mod-*`, nunca de entrada do usuário.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct IdModulo(pub &'static str);

impl IdModulo {
    /// Constrói a partir de um literal.
    #[must_use]
    pub const fn novo(id: &'static str) -> Self {
        Self(id)
    }

    /// O texto do id.
    #[must_use]
    pub const fn como_str(self) -> &'static str {
        self.0
    }
}

impl fmt::Display for IdModulo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

/// Um submódulo — a unidade de granularidade fina dentro de um módulo grande.
/// Ver `docs/04-pilar-modularidade.md` §2.1: é o que permite um MEI ligar só
/// `financeiro.receber` sem nunca ver `financeiro.centro_custo`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Submodulo {
    /// O id, único dentro do módulo (não precisa ser único no sistema todo).
    pub id: &'static str,
    /// O nome exibido.
    pub nome: &'static str,
    /// Se verdadeiro, não pode ser desligado individualmente — é o núcleo do módulo.
    pub essencial: bool,
    /// Outros submódulos (do mesmo módulo) que precisam estar ativos junto.
    pub depende_de: &'static [&'static str],
}

/// Uma entrada de menu contribuída por um módulo.
///
/// A sidebar inteira (`docs/12-ui-ux.md` §5) é a união ordenada por `peso` das entradas de
/// todos os módulos ativos — não existe um arquivo de menu escrito à mão.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EntradaMenu {
    /// Id estável da entrada, ex.: `"financeiro.pulso"`.
    pub id: &'static str,
    /// O texto exibido.
    pub rotulo: &'static str,
    /// O ícone.
    pub icone: Icone,
    /// Ordena a entrada dentro do menu — menor primeiro. O Pulso usa peso `0`
    /// (`docs/12-ui-ux.md` §5: "Financeiro é sempre o primeiro grupo").
    pub peso: u16,
    /// A permissão exigida para ver esta entrada.
    pub permissao: &'static str,
    /// Se definido, a entrada só aparece com este submódulo ativo.
    pub requer_submodulo: Option<&'static str>,
    /// O id da entrada pai, para montar hierarquia de menu (grupo → item).
    pub pai: Option<&'static str>,
}

/// Uma conta do plano que o módulo espera existir (por papel semântico) para poder postar
/// no Razão. Ver `docs/05-nucleo-financeiro.md` §5 (o receituário) e
/// `docs/contratos-internos.md` §6 regra 4.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ContaPadrao {
    /// O papel semântico exigido.
    pub papel: PapelConta,
    /// Se verdadeira, o módulo recusa ativar sem essa conta mapeada; se falsa, é usada só
    /// quando presente (ex.: `ReceitaHospedagem`, que só faz sentido com `hotelaria`).
    pub obrigatoria: bool,
}

/// A identidade e as capacidades de um módulo — o que ele é, do que depende, e o que
/// contribui (menu, permissões, contas). Ver `docs/contratos-internos.md` §4.
#[derive(Debug, Clone)]
pub struct Manifesto {
    /// O id estável.
    pub id: IdModulo,
    /// O nome exibido, ex.: `"Estoque"`.
    pub nome: &'static str,
    /// Versão semântica do módulo (major, minor, patch).
    pub versao: (u16, u16, u16),
    /// Descrição curta, para a tela de ativação de módulos.
    pub descricao: &'static str,
    /// O ícone do módulo na sidebar.
    pub icone: Icone,
    /// Dependência dura: ativar este módulo ativa esses primeiro (com confirmação do
    /// admin). Sem eles, o módulo não sobe.
    pub depende_de: &'static [IdModulo],
    /// Dependência mole: se presente, liga recursos extras; se ausente, degrada com
    /// elegância (`docs/04-pilar-modularidade.md` §5.3).
    pub melhora_com: &'static [IdModulo],
    /// Módulos que não podem estar ativos ao mesmo tempo que este.
    pub conflita_com: &'static [IdModulo],
    /// Os submódulos.
    pub submodulos: &'static [Submodulo],
    /// As permissões que este módulo declara.
    pub permissoes: &'static [Permissao],
    /// As entradas de menu contribuídas.
    pub menu: &'static [EntradaMenu],
    /// As contas do plano que o módulo precisa para postar no Razão.
    pub contas_requeridas: &'static [ContaPadrao],
    /// Eventos de domínio publicados por este módulo (`"modulo.substantivo_participio.v1"`).
    pub eventos_publicados: &'static [&'static str],
    /// Eventos de domínio que este módulo assina.
    pub eventos_assinados: &'static [&'static str],
}

/// Um problema de consistência interna do manifesto — pego antes do boot, não em produção.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ErroManifesto {
    /// Dois submódulos do mesmo módulo declaram o mesmo id.
    #[error("submódulo \"{0}\" duplicado no manifesto de \"{1}\"")]
    SubmoduloDuplicado(&'static str, &'static str),

    /// Um submódulo depende de um id de submódulo que não existe neste manifesto.
    #[error("submódulo \"{0}\" depende de \"{1}\", que não existe no manifesto de \"{2}\"")]
    DependenciaDeSubmoduloInexistente(&'static str, &'static str, &'static str),

    /// Uma entrada de menu ou permissão exige um submódulo que não existe.
    #[error("\"{0}\" exige o submódulo \"{1}\", que não existe no manifesto de \"{2}\"")]
    RequerSubmoduloInexistente(&'static str, &'static str, &'static str),

    /// Duas permissões do mesmo módulo têm a mesma chave.
    #[error("permissão \"{0}\" duplicada no manifesto de \"{1}\"")]
    PermissaoDuplicada(&'static str, &'static str),

    /// Uma permissão não começa com o id do próprio módulo — violaria
    /// `docs/15-convencoes-codigo.md` §6 e quebraria a extração de `Permissao::modulo`.
    #[error("permissão \"{0}\" não começa com \"{1}.\", no manifesto de \"{1}\"")]
    PermissaoForaDoNamespace(&'static str, &'static str),

    /// Uma entrada de menu referencia uma permissão que este manifesto não declara.
    #[error(
        "entrada de menu \"{0}\" referencia a permissão \"{1}\", ausente do manifesto de \"{2}\""
    )]
    MenuComPermissaoDesconhecida(&'static str, &'static str, &'static str),

    /// O módulo aparece na própria lista de dependências ou conflitos.
    #[error("\"{0}\" não pode depender de ou conflitar consigo mesmo")]
    AutoReferencia(&'static str),

    /// O mesmo módulo aparece em `depende_de` e `conflita_com` ao mesmo tempo.
    #[error("\"{0}\" declara \"{1}\" tanto como dependência quanto como conflito")]
    DependeEConflitaAoMesmoTempo(&'static str, &'static str),
}

impl Manifesto {
    /// Valida a consistência interna do manifesto — sem olhar para nenhum outro módulo.
    /// A validação entre módulos (grafo de dependências completo) é responsabilidade do
    /// `Registro`, que ainda depende de `cardeal-storage` para existir
    /// (`docs/17-roadmap.md`).
    ///
    /// # Errors
    /// A primeira inconsistência encontrada, como [`ErroManifesto`].
    pub fn validar(&self) -> Result<(), ErroManifesto> {
        let nome_modulo = self.id.como_str();

        let mut ids_submodulo = HashSet::new();
        for sub in self.submodulos {
            if !ids_submodulo.insert(sub.id) {
                return Err(ErroManifesto::SubmoduloDuplicado(sub.id, nome_modulo));
            }
        }
        for sub in self.submodulos {
            for dep in sub.depende_de {
                if !ids_submodulo.contains(dep) {
                    return Err(ErroManifesto::DependenciaDeSubmoduloInexistente(
                        sub.id,
                        dep,
                        nome_modulo,
                    ));
                }
            }
        }

        let checar_requer_submodulo = |origem: &'static str, requer: Option<&'static str>| {
            if let Some(sub) = requer {
                if !ids_submodulo.contains(sub) {
                    return Err(ErroManifesto::RequerSubmoduloInexistente(
                        origem,
                        sub,
                        nome_modulo,
                    ));
                }
            }
            Ok(())
        };

        let mut chaves_permissao = HashSet::new();
        for p in self.permissoes {
            if !chaves_permissao.insert(p.chave) {
                return Err(ErroManifesto::PermissaoDuplicada(p.chave, nome_modulo));
            }
            let prefixo = format!("{nome_modulo}.");
            if !p.chave.starts_with(&prefixo) {
                return Err(ErroManifesto::PermissaoForaDoNamespace(
                    p.chave,
                    nome_modulo,
                ));
            }
            checar_requer_submodulo(p.chave, p.requer_submodulo)?;
        }

        for entrada in self.menu {
            checar_requer_submodulo(entrada.id, entrada.requer_submodulo)?;
            if !chaves_permissao.contains(entrada.permissao) {
                return Err(ErroManifesto::MenuComPermissaoDesconhecida(
                    entrada.id,
                    entrada.permissao,
                    nome_modulo,
                ));
            }
        }

        for dep in self.depende_de {
            if *dep == self.id {
                return Err(ErroManifesto::AutoReferencia(nome_modulo));
            }
        }
        for conf in self.conflita_com {
            if *conf == self.id {
                return Err(ErroManifesto::AutoReferencia(nome_modulo));
            }
            if self.depende_de.contains(conf) {
                return Err(ErroManifesto::DependeEConflitaAoMesmoTempo(
                    nome_modulo,
                    conf.como_str(),
                ));
            }
        }

        Ok(())
    }

    /// Verdadeiro se o submódulo com este id existe e é essencial.
    #[must_use]
    pub fn submodulo_essencial(&self, id: &str) -> bool {
        self.submodulos.iter().any(|s| s.id == id && s.essencial)
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    const FINANCEIRO: IdModulo = IdModulo::novo("financeiro");

    fn manifesto_minimo() -> Manifesto {
        Manifesto {
            id: FINANCEIRO,
            nome: "Financeiro",
            versao: (0, 1, 0),
            descricao: "Caixa, contas a pagar e a receber.",
            icone: Icone::Dinheiro,
            depende_de: &[],
            melhora_com: &[],
            conflita_com: &[],
            submodulos: &[
                Submodulo {
                    id: "caixa",
                    nome: "Caixa",
                    essencial: true,
                    depende_de: &[],
                },
                Submodulo {
                    id: "conciliacao",
                    nome: "Conciliação",
                    essencial: false,
                    depende_de: &["caixa"],
                },
            ],
            permissoes: &[Permissao {
                chave: "financeiro.pulso.ver",
                descricao: "Ver o Pulso",
                risco: crate::permissao::Risco::Baixo,
                requer_submodulo: None,
            }],
            menu: &[EntradaMenu {
                id: "financeiro.pulso",
                rotulo: "Pulso",
                icone: Icone::Pulso,
                peso: 0,
                permissao: "financeiro.pulso.ver",
                requer_submodulo: None,
                pai: None,
            }],
            contas_requeridas: &[ContaPadrao {
                papel: PapelConta::Caixa,
                obrigatoria: true,
            }],
            eventos_publicados: &["financeiro.caixa_aberto.v1"],
            eventos_assinados: &[],
        }
    }

    #[test]
    fn manifesto_minimo_e_valido() {
        assert!(manifesto_minimo().validar().is_ok());
    }

    #[test]
    fn pega_submodulo_duplicado() {
        let mut m = manifesto_minimo();
        m.submodulos = &[
            Submodulo {
                id: "caixa",
                nome: "Caixa",
                essencial: true,
                depende_de: &[],
            },
            Submodulo {
                id: "caixa",
                nome: "Caixa de novo",
                essencial: false,
                depende_de: &[],
            },
        ];
        assert!(matches!(
            m.validar(),
            Err(ErroManifesto::SubmoduloDuplicado(..))
        ));
    }

    #[test]
    fn pega_dependencia_de_submodulo_inexistente() {
        let mut m = manifesto_minimo();
        m.submodulos = &[Submodulo {
            id: "cobranca",
            nome: "Cobrança",
            essencial: false,
            depende_de: &["nao_existe"],
        }];
        assert!(matches!(
            m.validar(),
            Err(ErroManifesto::DependenciaDeSubmoduloInexistente(..))
        ));
    }

    #[test]
    fn pega_permissao_fora_do_namespace() {
        let mut m = manifesto_minimo();
        m.permissoes = &[Permissao {
            chave: "estoque.saldo.ver", // não começa com "financeiro."
            descricao: "Errado",
            risco: crate::permissao::Risco::Baixo,
            requer_submodulo: None,
        }];
        assert!(matches!(
            m.validar(),
            Err(ErroManifesto::PermissaoForaDoNamespace(..))
        ));
    }

    #[test]
    fn pega_menu_com_permissao_desconhecida() {
        let mut m = manifesto_minimo();
        m.menu = &[EntradaMenu {
            id: "financeiro.receber",
            rotulo: "Contas a Receber",
            icone: Icone::Dinheiro,
            peso: 10,
            permissao: "financeiro.receber.ver", // não declarada em `permissoes`
            requer_submodulo: None,
            pai: None,
        }];
        assert!(matches!(
            m.validar(),
            Err(ErroManifesto::MenuComPermissaoDesconhecida(..))
        ));
    }

    #[test]
    fn pega_menu_que_exige_submodulo_inexistente() {
        let mut m = manifesto_minimo();
        m.menu = &[EntradaMenu {
            id: "financeiro.pulso",
            rotulo: "Pulso",
            icone: Icone::Pulso,
            peso: 0,
            permissao: "financeiro.pulso.ver",
            requer_submodulo: Some("nao_existe"),
            pai: None,
        }];
        assert!(matches!(
            m.validar(),
            Err(ErroManifesto::RequerSubmoduloInexistente(..))
        ));
    }

    #[test]
    fn pega_autoreferencia_e_conflito_com_dependencia() {
        const ESTOQUE: IdModulo = IdModulo::novo("estoque");

        let mut m = manifesto_minimo();
        m.depende_de = &[FINANCEIRO];
        assert!(matches!(m.validar(), Err(ErroManifesto::AutoReferencia(_))));

        m.depende_de = &[ESTOQUE];
        m.conflita_com = &[ESTOQUE];
        assert!(matches!(
            m.validar(),
            Err(ErroManifesto::DependeEConflitaAoMesmoTempo(..))
        ));
    }

    #[test]
    fn submodulo_essencial_funciona() {
        let m = manifesto_minimo();
        assert!(m.submodulo_essencial("caixa"));
        assert!(!m.submodulo_essencial("conciliacao"));
        assert!(!m.submodulo_essencial("nao_existe"));
    }
}
