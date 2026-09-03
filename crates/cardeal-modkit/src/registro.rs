//! Resolução do grafo de módulos — o que uma empresa efetivamente vê.
//!
//! `docs/04-pilar-modularidade.md` §2.2 e §4: a partir dos [`Manifesto`]s compilados e da
//! escolha do admin (`nucleo_modulo_ativo`), calcula-se em runtime o **conjunto efetivo**:
//! módulos em ordem topológica, submódulos ativos, permissões visíveis e o menu já
//! filtrado. Nada da UI é escrito à mão por perfil — tudo deriva daqui.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use crate::manifesto::{ContaPadrao, EntradaMenu, IdModulo, Manifesto};

/// O que pode dar errado ao registrar ou resolver módulos.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ErroRegistro {
    /// Dois módulos registrados com o mesmo id.
    #[error("módulo \"{0}\" registrado duas vezes")]
    ModuloDuplicado(&'static str),

    /// Um manifesto falhou na validação interna.
    #[error("o manifesto de \"{0}\" é inconsistente: {1}")]
    ManifestoInvalido(&'static str, String),

    /// Um módulo pedido (ou dependência dura) não está registrado.
    #[error("o módulo \"{0}\" não existe (pedido como dependência de \"{1}\")")]
    ModuloDesconhecido(String, String),

    /// O grafo de `depende_de` tem um ciclo.
    #[error("ciclo de dependências entre módulos, passando por \"{0}\"")]
    CicloDeDependencia(&'static str),

    /// Dois módulos ativos declaram conflito mútuo.
    #[error("\"{0}\" e \"{1}\" não podem estar ativos ao mesmo tempo")]
    Conflito(&'static str, &'static str),

    /// Um submódulo pedido não existe no módulo.
    #[error("o submódulo \"{submodulo}\" não existe em \"{modulo}\"")]
    SubmoduloDesconhecido {
        /// O módulo.
        modulo: String,
        /// O submódulo pedido.
        submodulo: String,
    },
}

/// A escolha do admin: quais módulos e submódulos a empresa quer ativos. Dependências duras
/// e submódulos essenciais são ligados automaticamente na resolução.
#[derive(Debug, Clone, Default)]
pub struct PedidoAtivacao {
    modulos: Vec<IdModulo>,
    submodulos: Vec<(&'static str, &'static str)>,
}

impl PedidoAtivacao {
    /// Um pedido vazio.
    #[must_use]
    pub fn nova() -> Self {
        Self::default()
    }

    /// Liga um módulo (e, na resolução, suas dependências duras).
    #[must_use]
    pub fn com_modulo(mut self, modulo: &'static str) -> Self {
        self.modulos.push(IdModulo::novo(modulo));
        self
    }

    /// Liga um submódulo específico de um módulo.
    #[must_use]
    pub fn com_submodulo(mut self, modulo: &'static str, submodulo: &'static str) -> Self {
        if !self.modulos.iter().any(|m| m.como_str() == modulo) {
            self.modulos.push(IdModulo::novo(modulo));
        }
        self.submodulos.push((modulo, submodulo));
        self
    }
}

/// O resultado da resolução: o que a empresa efetivamente enxerga.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConjuntoEfetivo {
    /// Módulos ativos em ordem topológica (dependências primeiro).
    pub modulos: Vec<IdModulo>,
    /// Submódulos ativos, por módulo.
    pub submodulos: BTreeMap<&'static str, BTreeSet<&'static str>>,
    /// Permissões visíveis no cadastro de papéis (o `requer_submodulo` está satisfeito).
    pub permissoes: BTreeSet<&'static str>,
    /// O menu já filtrado e ordenado por `peso`.
    pub menu: Vec<EntradaMenu>,
    /// As contas do plano que os módulos ativos exigem como obrigatórias.
    pub contas_requeridas: Vec<ContaPadrao>,
}

impl ConjuntoEfetivo {
    /// Verdadeiro se o módulo está ativo.
    #[must_use]
    pub fn modulo_ativo(&self, modulo: &str) -> bool {
        self.modulos.iter().any(|m| m.como_str() == modulo)
    }

    /// Verdadeiro se o submódulo está ativo.
    #[must_use]
    pub fn submodulo_ativo(&self, modulo: &str, submodulo: &str) -> bool {
        self.submodulos
            .get(modulo)
            .is_some_and(|s| s.contains(submodulo))
    }
}

/// Coleciona os manifestos compilados e resolve o conjunto efetivo por empresa.
#[derive(Debug, Default)]
pub struct RegistroModulos {
    manifestos: HashMap<&'static str, &'static Manifesto>,
}

impl RegistroModulos {
    /// Um registro vazio.
    #[must_use]
    pub fn novo() -> Self {
        Self::default()
    }

    /// Registra um manifesto, validando sua consistência interna.
    ///
    /// # Errors
    /// [`ErroRegistro::ModuloDuplicado`], [`ErroRegistro::ManifestoInvalido`].
    pub fn registrar(&mut self, manifesto: &'static Manifesto) -> Result<(), ErroRegistro> {
        let id = manifesto.id.como_str();
        manifesto
            .validar()
            .map_err(|e| ErroRegistro::ManifestoInvalido(id, e.to_string()))?;
        if self.manifestos.insert(id, manifesto).is_some() {
            return Err(ErroRegistro::ModuloDuplicado(id));
        }
        Ok(())
    }

    /// Quantos módulos estão registrados.
    #[must_use]
    pub fn total(&self) -> usize {
        self.manifestos.len()
    }

    /// A ordem topológica de **todos** os módulos registrados — a ordem de aplicação de
    /// migrações (`docs/06-modelo-de-dados.md` §4, regra 5).
    ///
    /// # Errors
    /// [`ErroRegistro::CicloDeDependencia`], [`ErroRegistro::ModuloDesconhecido`].
    pub fn ordem_de_migracao(&self) -> Result<Vec<IdModulo>, ErroRegistro> {
        let todos: Vec<&'static str> = self.manifestos.keys().copied().collect();
        self.ordenar(&todos)
    }

    /// Resolve o conjunto efetivo a partir do pedido do admin.
    ///
    /// # Errors
    /// Ver [`ErroRegistro`].
    pub fn resolver(&self, pedido: &PedidoAtivacao) -> Result<ConjuntoEfetivo, ErroRegistro> {
        // 1. Fecho transitivo das dependências duras.
        let mut ativos: HashSet<&'static str> = HashSet::new();
        for m in &pedido.modulos {
            self.ativar_com_dependencias(m.como_str(), &mut ativos)?;
        }

        // 2. Conflitos.
        for &nome in &ativos {
            let m = self.manifestos[nome];
            for conf in m.conflita_com {
                if ativos.contains(conf.como_str()) {
                    let (a, b) = if nome < conf.como_str() {
                        (nome, conf.como_str())
                    } else {
                        (conf.como_str(), nome)
                    };
                    return Err(ErroRegistro::Conflito(a, b));
                }
            }
        }

        // 3. Ordem topológica dos ativos.
        let ativos_vec: Vec<&'static str> = ativos.iter().copied().collect();
        let ordem = self.ordenar(&ativos_vec)?;

        // 4. Submódulos: essenciais + pedidos, fechados sobre `depende_de`.
        let mut submodulos: BTreeMap<&'static str, BTreeSet<&'static str>> = BTreeMap::new();
        for &nome in &ativos {
            let m = self.manifestos[nome];
            let mut ligados: BTreeSet<&'static str> = m
                .submodulos
                .iter()
                .filter(|s| s.essencial)
                .map(|s| s.id)
                .collect();
            for (mod_ped, sub_ped) in &pedido.submodulos {
                if *mod_ped == nome {
                    if !m.submodulos.iter().any(|s| s.id == *sub_ped) {
                        return Err(ErroRegistro::SubmoduloDesconhecido {
                            modulo: nome.to_string(),
                            submodulo: (*sub_ped).to_string(),
                        });
                    }
                    ligados.insert(sub_ped);
                }
            }
            fechar_submodulos(m, &mut ligados);
            submodulos.insert(nome, ligados);
        }

        // 5. Permissões visíveis.
        let mut permissoes: BTreeSet<&'static str> = BTreeSet::new();
        for &nome in &ativos {
            let m = self.manifestos[nome];
            let subs = &submodulos[nome];
            for p in m.permissoes {
                if p.requer_submodulo.is_none_or(|s| subs.contains(s)) {
                    permissoes.insert(p.chave);
                }
            }
        }

        // 6. Menu filtrado e ordenado.
        let mut menu: Vec<EntradaMenu> = Vec::new();
        for &nome in &ativos {
            let m = self.manifestos[nome];
            let subs = &submodulos[nome];
            for e in m.menu {
                let sub_ok = e.requer_submodulo.is_none_or(|s| subs.contains(s));
                if sub_ok && permissoes.contains(e.permissao) {
                    menu.push(*e);
                }
            }
        }
        menu.sort_by_key(|e| (e.peso, e.id));

        // 7. Contas obrigatórias.
        let mut contas_requeridas: Vec<ContaPadrao> = Vec::new();
        for &nome in &ordem_como_str(&ordem) {
            for c in self.manifestos[nome].contas_requeridas {
                if c.obrigatoria && !contas_requeridas.iter().any(|x| x.papel == c.papel) {
                    contas_requeridas.push(*c);
                }
            }
        }

        Ok(ConjuntoEfetivo {
            modulos: ordem,
            submodulos,
            permissoes,
            menu,
            contas_requeridas,
        })
    }

    fn ativar_com_dependencias(
        &self,
        nome: &'static str,
        ativos: &mut HashSet<&'static str>,
    ) -> Result<(), ErroRegistro> {
        if ativos.contains(nome) {
            return Ok(());
        }
        let Some(m) = self.manifestos.get(nome) else {
            return Err(ErroRegistro::ModuloDesconhecido(
                nome.to_string(),
                "pedido de ativação".to_string(),
            ));
        };
        ativos.insert(nome);
        for dep in m.depende_de {
            if !self.manifestos.contains_key(dep.como_str()) {
                return Err(ErroRegistro::ModuloDesconhecido(
                    dep.como_str().to_string(),
                    nome.to_string(),
                ));
            }
            self.ativar_com_dependencias(dep.como_str(), ativos)?;
        }
        Ok(())
    }

    fn ordenar(&self, nomes: &[&'static str]) -> Result<Vec<IdModulo>, ErroRegistro> {
        let mut ordem: Vec<IdModulo> = Vec::with_capacity(nomes.len());
        let mut feitos: HashSet<&'static str> = HashSet::new();
        let mut visitando: HashSet<&'static str> = HashSet::new();
        let mut nomes = nomes.to_vec();
        nomes.sort_unstable();
        for nome in nomes {
            self.visitar(nome, &mut feitos, &mut visitando, &mut ordem)?;
        }
        Ok(ordem)
    }

    fn visitar(
        &self,
        nome: &'static str,
        feitos: &mut HashSet<&'static str>,
        visitando: &mut HashSet<&'static str>,
        ordem: &mut Vec<IdModulo>,
    ) -> Result<(), ErroRegistro> {
        if feitos.contains(nome) {
            return Ok(());
        }
        if !visitando.insert(nome) {
            return Err(ErroRegistro::CicloDeDependencia(nome));
        }
        let m = self
            .manifestos
            .get(nome)
            .ok_or_else(|| ErroRegistro::ModuloDesconhecido(nome.to_string(), String::new()))?;
        let mut deps: Vec<&'static str> = m.depende_de.iter().map(|d| d.como_str()).collect();
        deps.sort_unstable();
        for dep in deps {
            self.visitar(dep, feitos, visitando, ordem)?;
        }
        visitando.remove(nome);
        feitos.insert(nome);
        ordem.push(m.id);
        Ok(())
    }
}

fn ordem_como_str(ordem: &[IdModulo]) -> Vec<&'static str> {
    ordem.iter().map(|m| m.como_str()).collect()
}

/// Liga, iterativamente, os submódulos de que os já ligados dependem.
fn fechar_submodulos(m: &Manifesto, ligados: &mut BTreeSet<&'static str>) {
    loop {
        let mut novos: Vec<&'static str> = Vec::new();
        for s in m.submodulos {
            if ligados.contains(s.id) {
                for dep in s.depende_de {
                    if !ligados.contains(dep) {
                        novos.push(dep);
                    }
                }
            }
        }
        if novos.is_empty() {
            break;
        }
        ligados.extend(novos);
    }
}

#[cfg(test)]
mod testes {
    use cardeal_ledger::PapelConta;

    use super::*;
    use crate::manifesto::Submodulo;
    use crate::permissao::{Permissao, Risco};
    use crate::Icone;

    const fn manifesto_const(
        id: &'static str,
        depende_de: &'static [IdModulo],
        conflita_com: &'static [IdModulo],
        submodulos: &'static [Submodulo],
        permissoes: &'static [Permissao],
        menu: &'static [EntradaMenu],
    ) -> Manifesto {
        Manifesto {
            id: IdModulo::novo(id),
            nome: id,
            versao: (0, 1, 0),
            descricao: "x",
            icone: Icone::Config,
            depende_de,
            melhora_com: &[],
            conflita_com,
            submodulos,
            permissoes,
            menu,
            contas_requeridas: &[],
            eventos_publicados: &[],
            eventos_assinados: &[],
        }
    }

    static FINANCEIRO: Manifesto = {
        static SUBS: &[Submodulo] = &[
            Submodulo {
                id: "receber",
                nome: "R",
                essencial: true,
                depende_de: &[],
            },
            Submodulo {
                id: "bancos",
                nome: "B",
                essencial: false,
                depende_de: &[],
            },
            Submodulo {
                id: "conciliacao",
                nome: "C",
                essencial: false,
                depende_de: &["bancos"],
            },
        ];
        static PERMS: &[Permissao] = &[
            Permissao {
                chave: "financeiro.receber.ver",
                descricao: "x",
                risco: Risco::Baixo,
                requer_submodulo: Some("receber"),
            },
            Permissao {
                chave: "financeiro.conciliacao.ver",
                descricao: "x",
                risco: Risco::Baixo,
                requer_submodulo: Some("conciliacao"),
            },
        ];
        static MENU: &[EntradaMenu] = &[
            EntradaMenu {
                id: "financeiro.receber",
                rotulo: "Receber",
                icone: Icone::Dinheiro,
                peso: 10,
                permissao: "financeiro.receber.ver",
                requer_submodulo: Some("receber"),
                pai: None,
            },
            EntradaMenu {
                id: "financeiro.conciliacao",
                rotulo: "Conciliação",
                icone: Icone::Conciliar,
                peso: 20,
                permissao: "financeiro.conciliacao.ver",
                requer_submodulo: Some("conciliacao"),
                pai: None,
            },
        ];
        manifesto_const("financeiro", &[], &[], SUBS, PERMS, MENU)
    };

    static VENDAS: Manifesto = {
        static PERMS: &[Permissao] = &[Permissao {
            chave: "vendas.pedido.ver",
            descricao: "x",
            risco: Risco::Baixo,
            requer_submodulo: None,
        }];
        static MENU: &[EntradaMenu] = &[EntradaMenu {
            id: "vendas.pedidos",
            rotulo: "Pedidos",
            icone: Icone::Carrinho,
            peso: 40,
            permissao: "vendas.pedido.ver",
            requer_submodulo: None,
            pai: None,
        }];
        manifesto_const(
            "vendas",
            &[IdModulo::novo("financeiro")],
            &[],
            &[],
            PERMS,
            MENU,
        )
    };

    static CONFLITA_A: Manifesto = manifesto_const(
        "conflita_a",
        &[],
        &[IdModulo::novo("conflita_b")],
        &[],
        &[],
        &[],
    );
    static CONFLITA_B: Manifesto = manifesto_const(
        "conflita_b",
        &[],
        &[IdModulo::novo("conflita_a")],
        &[],
        &[],
        &[],
    );

    static COM_CONTAS: Manifesto = {
        static CR: &[ContaPadrao] = &[
            ContaPadrao {
                papel: PapelConta::Caixa,
                obrigatoria: true,
            },
            ContaPadrao {
                papel: PapelConta::Bancos,
                obrigatoria: false,
            },
        ];
        let mut m = manifesto_const("com_contas", &[], &[], &[], &[], &[]);
        m.contas_requeridas = CR;
        m
    };

    fn registro_padrao() -> RegistroModulos {
        let mut r = RegistroModulos::novo();
        r.registrar(&FINANCEIRO).unwrap();
        r.registrar(&VENDAS).unwrap();
        r
    }

    #[test]
    fn ativar_vendas_liga_financeiro_como_dependencia() {
        let r = registro_padrao();
        let efetivo = r
            .resolver(&PedidoAtivacao::nova().com_modulo("vendas"))
            .unwrap();
        assert!(efetivo.modulo_ativo("financeiro"));
        assert!(efetivo.modulo_ativo("vendas"));
        // Ordem topológica: financeiro antes de vendas.
        let pos = |m: &str| {
            efetivo
                .modulos
                .iter()
                .position(|x| x.como_str() == m)
                .unwrap()
        };
        assert!(pos("financeiro") < pos("vendas"));
    }

    #[test]
    fn submodulo_essencial_sempre_liga_e_o_menu_dele_aparece() {
        let r = registro_padrao();
        let efetivo = r
            .resolver(&PedidoAtivacao::nova().com_modulo("financeiro"))
            .unwrap();
        assert!(efetivo.submodulo_ativo("financeiro", "receber"));
        assert!(!efetivo.submodulo_ativo("financeiro", "conciliacao"));
        // "receber" aparece no menu; "conciliação" (submódulo off) não.
        let ids: Vec<&str> = efetivo.menu.iter().map(|e| e.id).collect();
        assert!(ids.contains(&"financeiro.receber"));
        assert!(!ids.contains(&"financeiro.conciliacao"));
        // Permissão de conciliação não é visível.
        assert!(efetivo.permissoes.contains("financeiro.receber.ver"));
        assert!(!efetivo.permissoes.contains("financeiro.conciliacao.ver"));
    }

    #[test]
    fn ligar_conciliacao_liga_bancos_automaticamente() {
        let r = registro_padrao();
        let efetivo = r
            .resolver(&PedidoAtivacao::nova().com_submodulo("financeiro", "conciliacao"))
            .unwrap();
        assert!(efetivo.submodulo_ativo("financeiro", "bancos"));
        assert!(efetivo.submodulo_ativo("financeiro", "conciliacao"));
        assert!(efetivo.permissoes.contains("financeiro.conciliacao.ver"));
        assert!(efetivo
            .menu
            .iter()
            .any(|e| e.id == "financeiro.conciliacao"));
    }

    #[test]
    fn conflito_entre_modulos_ativos_e_recusado() {
        let mut r = RegistroModulos::novo();
        r.registrar(&CONFLITA_A).unwrap();
        r.registrar(&CONFLITA_B).unwrap();
        let erro = r
            .resolver(
                &PedidoAtivacao::nova()
                    .com_modulo("conflita_a")
                    .com_modulo("conflita_b"),
            )
            .unwrap_err();
        assert!(matches!(erro, ErroRegistro::Conflito(..)));
    }

    #[test]
    fn modulo_desconhecido_e_erro() {
        let r = registro_padrao();
        let erro = r
            .resolver(&PedidoAtivacao::nova().com_modulo("inexistente"))
            .unwrap_err();
        assert!(matches!(erro, ErroRegistro::ModuloDesconhecido(..)));
    }

    #[test]
    fn registrar_duas_vezes_o_mesmo_id_falha() {
        let mut r = RegistroModulos::novo();
        r.registrar(&FINANCEIRO).unwrap();
        assert!(matches!(
            r.registrar(&FINANCEIRO),
            Err(ErroRegistro::ModuloDuplicado("financeiro"))
        ));
    }

    #[test]
    fn ordem_de_migracao_poe_dependencia_antes() {
        let r = registro_padrao();
        let ordem = r.ordem_de_migracao().unwrap();
        let pos = |m: &str| ordem.iter().position(|x| x.como_str() == m).unwrap();
        assert!(pos("financeiro") < pos("vendas"));
    }

    #[test]
    fn contas_obrigatorias_dos_modulos_ativos_sao_reunidas() {
        let mut r = RegistroModulos::novo();
        r.registrar(&COM_CONTAS).unwrap();
        let efetivo = r
            .resolver(&PedidoAtivacao::nova().com_modulo("com_contas"))
            .unwrap();
        assert_eq!(efetivo.contas_requeridas.len(), 1);
        assert_eq!(efetivo.contas_requeridas[0].papel, PapelConta::Caixa);
    }
}
