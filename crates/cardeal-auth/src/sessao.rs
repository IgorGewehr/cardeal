//! Sessão e autorização — a fusão de "quem é", "onde pode agir" e "o que pode fazer".
//!
//! Ver `docs/08-seguranca-permissoes.md` §2 e §3. Uma [`Sessao`] é uma pessoa, num
//! dispositivo, numa empresa, por um tempo. Ela carrega o conjunto **já resolvido** de
//! permissões e limites — a interseção dos papéis do usuário com o que a empresa tem ativo
//! (o [`ConjuntoEfetivo`] de `cardeal-modkit`) — e um [`Escopo`] (§3.3). A função livre
//! [`autorizar`] é o único ponto de verdade: um comando só passa se a sessão concede a
//! permissão **e** o escopo dela abrange o recurso.
//!
//! [`ConjuntoEfetivo`]: https://docs.rs/cardeal-modkit

use std::collections::{BTreeMap, BTreeSet};

use cardeal_kernel::{Id, Instante};

use crate::erros::ErroAuth;
use crate::limite::ValorLimite;
use crate::papel::Papel;
use crate::Escopo;

/// Duração padrão do token de acesso — `docs/08 §4.2`: vida curta, renovada por token
/// rotativo. No PDV a sessão do terminal dura o turno; a do operador é esta camada leve.
pub const DURACAO_PADRAO_SEGUNDOS: i64 = 15 * 60;

/// O que um usuário pode fazer numa empresa, já consolidado a partir dos seus papéis.
///
/// É o resultado de cruzar os papéis atribuídos ao usuário com o conjunto de permissões
/// que a empresa efetivamente enxerga — a filtragem por `ConjuntoEfetivo` é feita por quem
/// constrói isto (a camada que tem o registro de módulos), passando apenas os papéis já
/// pertinentes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AutorizacoesEfetivas {
    concedidas: BTreeSet<String>,
    limites: BTreeMap<String, ValorLimite>,
}

impl AutorizacoesEfetivas {
    /// Consolida os papéis de um usuário: união das permissões, e o teto mais permissivo
    /// de cada limite (quem acumula papéis fica com o maior).
    #[must_use]
    pub fn consolidar<'a>(papeis: impl IntoIterator<Item = &'a Papel>) -> Self {
        let mut efetivas = Self::default();
        for papel in papeis {
            for chave in &papel.permissoes {
                efetivas.concedidas.insert(chave.clone());
            }
            for (chave, valor) in &papel.limites {
                efetivas
                    .limites
                    .entry(chave.clone())
                    .and_modify(|atual| *atual = atual.mais_permissivo(*valor))
                    .or_insert(*valor);
            }
        }
        efetivas
    }

    /// Mantém só as permissões presentes em `visiveis` — a interseção com o
    /// `ConjuntoEfetivo` da empresa. Um limite só sobrevive se sua chave-base ainda tem
    /// permissão ou se é uma chave de limite pura (não removida aqui).
    #[must_use]
    pub fn restringir_a<'a>(mut self, visiveis: impl IntoIterator<Item = &'a str>) -> Self {
        let visiveis: BTreeSet<&str> = visiveis.into_iter().collect();
        self.concedidas.retain(|c| visiveis.contains(c.as_str()));
        self
    }

    /// Verdadeiro se a permissão está concedida.
    #[must_use]
    pub fn concede(&self, permissao: &str) -> bool {
        self.concedidas.contains(permissao)
    }

    /// O limite associado a uma chave, se houver.
    #[must_use]
    pub fn limite(&self, chave: &str) -> Option<ValorLimite> {
        self.limites.get(chave).copied()
    }

    /// Quantas permissões concede.
    #[must_use]
    pub fn total(&self) -> usize {
        self.concedidas.len()
    }

    /// Itera as chaves concedidas, em ordem.
    pub fn iter(&self) -> impl Iterator<Item = &str> {
        self.concedidas.iter().map(String::as_str)
    }
}

/// Os dados para abrir uma sessão.
#[derive(Debug, Clone)]
pub struct EmissaoSessao {
    /// O usuário autenticado.
    pub usuario: Id,
    /// O dispositivo registrado de onde parte.
    pub dispositivo: Id,
    /// O escopo em que a sessão age (contém a empresa).
    pub escopo: Escopo,
    /// As autorizações já consolidadas para esse usuário nessa empresa.
    pub autorizacoes: AutorizacoesEfetivas,
    /// Quando a sessão é emitida.
    pub emitida_em: Instante,
    /// Duração do token, em segundos.
    pub duracao_segundos: i64,
}

impl EmissaoSessao {
    /// Emissão com a duração padrão (`docs/08 §4.2`).
    #[must_use]
    pub fn padrao(
        usuario: Id,
        dispositivo: Id,
        escopo: Escopo,
        autorizacoes: AutorizacoesEfetivas,
        emitida_em: Instante,
    ) -> Self {
        Self {
            usuario,
            dispositivo,
            escopo,
            autorizacoes,
            emitida_em,
            duracao_segundos: DURACAO_PADRAO_SEGUNDOS,
        }
    }
}

/// Uma sessão viva: uma pessoa, num dispositivo, numa empresa, por um tempo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sessao {
    /// Identidade da sessão.
    pub id: Id,
    /// O usuário.
    pub usuario: Id,
    /// O dispositivo — um token válido em outro dispositivo é rejeitado (`docs/08 §2`).
    pub dispositivo: Id,
    /// O escopo em que age.
    pub escopo: Escopo,
    /// Quando foi emitida.
    pub emitida_em: Instante,
    /// Quando expira o token de acesso.
    pub expira_em: Instante,
    /// Quando foi encerrada (logout ou revogação), se foi.
    pub encerrada_em: Option<Instante>,
    autorizacoes: AutorizacoesEfetivas,
}

impl Sessao {
    /// Abre uma sessão a partir da emissão.
    #[must_use]
    pub fn abrir(emissao: EmissaoSessao) -> Self {
        Self {
            id: Id::novo(),
            usuario: emissao.usuario,
            dispositivo: emissao.dispositivo,
            escopo: emissao.escopo,
            emitida_em: emissao.emitida_em,
            expira_em: emissao.emitida_em.mais_segundos(emissao.duracao_segundos),
            encerrada_em: None,
            autorizacoes: emissao.autorizacoes,
        }
    }

    /// A empresa da sessão.
    #[must_use]
    pub fn empresa(&self) -> Id {
        self.escopo.empresa
    }

    /// Verdadeiro se a sessão ainda vale em `agora`: não encerrada e antes de expirar.
    #[must_use]
    pub fn esta_valida(&self, agora: Instante) -> bool {
        self.encerrada_em.is_none() && agora < self.expira_em
    }

    /// Renova o token de acesso, estendendo a expiração a partir de `agora`. Não revive
    /// uma sessão já encerrada.
    ///
    /// # Errors
    /// [`ErroAuth::SessaoEncerrada`] se a sessão foi encerrada.
    pub fn renovar(&mut self, agora: Instante, duracao_segundos: i64) -> Result<(), ErroAuth> {
        if self.encerrada_em.is_some() {
            return Err(ErroAuth::SessaoEncerrada);
        }
        self.expira_em = agora.mais_segundos(duracao_segundos);
        Ok(())
    }

    /// Encerra a sessão (logout ou revogação). Idempotente.
    pub fn encerrar(&mut self, agora: Instante) {
        self.encerrada_em.get_or_insert(agora);
    }

    /// As autorizações consolidadas da sessão.
    #[must_use]
    pub fn autorizacoes(&self) -> &AutorizacoesEfetivas {
        &self.autorizacoes
    }

    /// O limite de uma chave, para o comando decidir entre barrar e escalar ao supervisor.
    #[must_use]
    pub fn limite(&self, chave: &str) -> Option<ValorLimite> {
        self.autorizacoes.limite(chave)
    }
}

/// Autoriza uma ação: a sessão precisa **conceder a permissão** e o **escopo dela abranger
/// o recurso**. É o único ponto de verificação (`docs/08 §3.5`: o servidor é a fonte de
/// verdade).
///
/// A validade temporal da sessão é responsabilidade da borda de transporte (validação do
/// token), não desta função — mas por segurança de defesa em profundidade, uma sessão
/// encerrada é recusada aqui também.
///
/// ```
/// use cardeal_kernel::{Id, Instante};
/// use cardeal_auth::{autorizar, AutorizacoesEfetivas, EmissaoSessao, Escopo, Papel, Sessao};
///
/// let empresa = Id::novo();
/// let filial_1 = Id::novo();
/// let filial_2 = Id::novo();
///
/// let papel = Papel::novo(empresa, "Caixa").com_permissao("financeiro.caixa.abrir");
/// let auth = AutorizacoesEfetivas::consolidar([&papel]);
///
/// let sessao = Sessao::abrir(EmissaoSessao::padrao(
///     Id::novo(),
///     Id::novo(),
///     Escopo::empresa_inteira(empresa).com_filial(filial_1),
///     auth,
///     Instante::EPOCA,
/// ));
///
/// // Concede a permissão e o recurso é da filial 1: passa.
/// let recurso_f1 = Escopo::empresa_inteira(empresa).com_filial(filial_1);
/// assert!(autorizar(&sessao, "financeiro.caixa.abrir", &recurso_f1).is_ok());
///
/// // Mesmo com a permissão, um recurso da filial 2 está fora do escopo.
/// let recurso_f2 = Escopo::empresa_inteira(empresa).com_filial(filial_2);
/// assert!(autorizar(&sessao, "financeiro.caixa.abrir", &recurso_f2).is_err());
///
/// // Permissão não concedida: recusa.
/// assert!(autorizar(&sessao, "razao.estornar", &recurso_f1).is_err());
/// ```
///
/// # Errors
/// - [`ErroAuth::SessaoEncerrada`] se a sessão já foi encerrada.
/// - [`ErroAuth::ForaDoEscopo`] se o escopo da sessão não abrange o recurso.
/// - [`ErroAuth::SemPermissao`] se a sessão não concede a permissão.
pub fn autorizar(sessao: &Sessao, permissao: &str, recurso: &Escopo) -> Result<(), ErroAuth> {
    if sessao.encerrada_em.is_some() {
        return Err(ErroAuth::SessaoEncerrada);
    }
    if !sessao.escopo.abrange(recurso) {
        return Err(ErroAuth::ForaDoEscopo);
    }
    if !sessao.autorizacoes.concede(permissao) {
        return Err(ErroAuth::SemPermissao {
            permissao: permissao.to_string(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod testes {
    use cardeal_kernel::Percentual;

    use super::*;

    fn papel_caixa(empresa: Id) -> Papel {
        Papel::novo(empresa, "Caixa")
            .com_permissao("financeiro.caixa.abrir")
            .com_permissao("vendas.venda.registrar")
            .com_limite(
                "vendas.desconto_maximo",
                ValorLimite::Percentual(Percentual::pontos(5)),
            )
    }

    fn papel_gerente(empresa: Id) -> Papel {
        Papel::novo(empresa, "Gerente")
            .com_permissao("vendas.venda.registrar")
            .com_permissao("vendas.pedido.faturar")
            .com_limite(
                "vendas.desconto_maximo",
                ValorLimite::Percentual(Percentual::pontos(20)),
            )
    }

    fn sessao_de(papeis: &[&Papel], escopo: Escopo) -> Sessao {
        Sessao::abrir(EmissaoSessao::padrao(
            Id::novo(),
            Id::novo(),
            escopo,
            AutorizacoesEfetivas::consolidar(papeis.iter().copied()),
            Instante::EPOCA,
        ))
    }

    #[test]
    fn autoriza_quando_concede_e_escopo_abrange() {
        let empresa = Id::novo();
        let p = papel_caixa(empresa);
        let sessao = sessao_de(&[&p], Escopo::empresa_inteira(empresa));
        assert!(autorizar(
            &sessao,
            "financeiro.caixa.abrir",
            &Escopo::empresa_inteira(empresa)
        )
        .is_ok());
    }

    #[test]
    fn recusa_permissao_nao_concedida() {
        let empresa = Id::novo();
        let p = papel_caixa(empresa);
        let sessao = sessao_de(&[&p], Escopo::empresa_inteira(empresa));
        let erro = autorizar(
            &sessao,
            "vendas.pedido.faturar",
            &Escopo::empresa_inteira(empresa),
        )
        .unwrap_err();
        assert!(matches!(erro, ErroAuth::SemPermissao { .. }));
    }

    #[test]
    fn recusa_fora_do_escopo_mesmo_com_permissao() {
        let empresa = Id::novo();
        let filial_1 = Id::novo();
        let filial_2 = Id::novo();
        let p = papel_caixa(empresa);
        let sessao = sessao_de(&[&p], Escopo::empresa_inteira(empresa).com_filial(filial_1));
        let erro = autorizar(
            &sessao,
            "financeiro.caixa.abrir",
            &Escopo::empresa_inteira(empresa).com_filial(filial_2),
        )
        .unwrap_err();
        assert!(matches!(erro, ErroAuth::ForaDoEscopo));
    }

    #[test]
    fn sessao_encerrada_nao_autoriza() {
        let empresa = Id::novo();
        let p = papel_caixa(empresa);
        let mut sessao = sessao_de(&[&p], Escopo::empresa_inteira(empresa));
        sessao.encerrar(Instante::EPOCA.mais_segundos(10));
        let erro = autorizar(
            &sessao,
            "financeiro.caixa.abrir",
            &Escopo::empresa_inteira(empresa),
        )
        .unwrap_err();
        assert!(matches!(erro, ErroAuth::SessaoEncerrada));
    }

    #[test]
    fn validade_temporal_respeita_a_duracao() {
        let empresa = Id::novo();
        let p = papel_caixa(empresa);
        let sessao = sessao_de(&[&p], Escopo::empresa_inteira(empresa));
        assert!(sessao.esta_valida(Instante::EPOCA));
        assert!(sessao.esta_valida(Instante::EPOCA.mais_segundos(DURACAO_PADRAO_SEGUNDOS - 1)));
        assert!(!sessao.esta_valida(Instante::EPOCA.mais_segundos(DURACAO_PADRAO_SEGUNDOS + 1)));
    }

    #[test]
    fn renovar_estende_a_expiracao() {
        let empresa = Id::novo();
        let p = papel_caixa(empresa);
        let mut sessao = sessao_de(&[&p], Escopo::empresa_inteira(empresa));
        let daqui = Instante::EPOCA.mais_segundos(600);
        sessao.renovar(daqui, DURACAO_PADRAO_SEGUNDOS).unwrap();
        assert!(sessao.esta_valida(daqui.mais_segundos(DURACAO_PADRAO_SEGUNDOS - 1)));
    }

    #[test]
    fn renovar_sessao_encerrada_falha() {
        let empresa = Id::novo();
        let p = papel_caixa(empresa);
        let mut sessao = sessao_de(&[&p], Escopo::empresa_inteira(empresa));
        sessao.encerrar(Instante::EPOCA);
        assert!(matches!(
            sessao.renovar(Instante::EPOCA.mais_segundos(1), DURACAO_PADRAO_SEGUNDOS),
            Err(ErroAuth::SessaoEncerrada)
        ));
    }

    #[test]
    fn consolidar_une_permissoes_e_pega_o_maior_limite() {
        let empresa = Id::novo();
        let caixa = papel_caixa(empresa);
        let gerente = papel_gerente(empresa);
        let auth = AutorizacoesEfetivas::consolidar([&caixa, &gerente]);
        assert!(auth.concede("financeiro.caixa.abrir"));
        assert!(auth.concede("vendas.pedido.faturar"));
        assert_eq!(
            auth.limite("vendas.desconto_maximo"),
            Some(ValorLimite::Percentual(Percentual::pontos(20)))
        );
    }

    #[test]
    fn restringir_a_remove_o_que_a_empresa_nao_enxerga() {
        let empresa = Id::novo();
        let caixa = papel_caixa(empresa);
        let auth =
            AutorizacoesEfetivas::consolidar([&caixa]).restringir_a(["financeiro.caixa.abrir"]);
        assert!(auth.concede("financeiro.caixa.abrir"));
        assert!(!auth.concede("vendas.venda.registrar"));
    }
}
