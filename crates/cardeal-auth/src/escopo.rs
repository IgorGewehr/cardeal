//! Escopo de autorização — a parte ABAC da permissão.
//!
//! Ver `docs/08-seguranca-permissoes.md` §3.3: uma permissão sozinha não basta. "Gerente
//! da Filial 2 não vê o caixa da Filial 1", "vendedor só vê os clientes da própria
//! carteira", "operador só opera o caixa em que abriu turno" — os três casos são a mesma
//! regra: um campo `None` no escopo autorizado significa "sem restrição nessa dimensão";
//! um campo `Some` exige igualdade exata com o escopo do recurso.

use cardeal_kernel::Id;

/// A que fatia do negócio uma autorização (ou um recurso) pertence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Escopo {
    /// A empresa — sempre exigida, nunca `None`.
    pub empresa: Id,
    /// A filial, quando o recurso pertence a uma específica.
    pub filial: Option<Id>,
    /// O caixa, para operações de PDV.
    pub caixa: Option<Id>,
    /// O centro de custo.
    pub centro_custo: Option<Id>,
}

impl Escopo {
    /// O escopo mais amplo possível dentro de uma empresa: nenhuma restrição de filial,
    /// caixa ou centro de custo. É o escopo de um administrador.
    #[must_use]
    pub const fn empresa_inteira(empresa: Id) -> Self {
        Self {
            empresa,
            filial: None,
            caixa: None,
            centro_custo: None,
        }
    }

    /// Restringe a uma filial.
    #[must_use]
    pub const fn com_filial(mut self, filial: Id) -> Self {
        self.filial = Some(filial);
        self
    }

    /// Restringe a um caixa.
    #[must_use]
    pub const fn com_caixa(mut self, caixa: Id) -> Self {
        self.caixa = Some(caixa);
        self
    }

    /// Restringe a um centro de custo.
    #[must_use]
    pub const fn com_centro_custo(mut self, centro_custo: Id) -> Self {
        self.centro_custo = Some(centro_custo);
        self
    }

    /// Verdadeiro se **este** escopo (a autorização de um usuário) abrange o `recurso`
    /// informado — ou seja, se o usuário autorizado neste escopo pode agir sobre um
    /// recurso que vive naquele escopo.
    ///
    /// ```
    /// use cardeal_kernel::Id;
    /// use cardeal_auth::Escopo;
    ///
    /// let empresa = Id::novo();
    /// let filial_2 = Id::novo();
    /// let filial_1 = Id::novo();
    ///
    /// let gerente_filial_2 = Escopo::empresa_inteira(empresa).com_filial(filial_2);
    /// let caixa_da_filial_1 = Escopo::empresa_inteira(empresa).com_filial(filial_1);
    /// let caixa_da_filial_2 = Escopo::empresa_inteira(empresa).com_filial(filial_2);
    ///
    /// assert!(!gerente_filial_2.abrange(&caixa_da_filial_1));
    /// assert!(gerente_filial_2.abrange(&caixa_da_filial_2));
    ///
    /// // Um administrador (sem restrição de filial) abrange qualquer filial.
    /// let admin = Escopo::empresa_inteira(empresa);
    /// assert!(admin.abrange(&caixa_da_filial_1));
    /// assert!(admin.abrange(&caixa_da_filial_2));
    /// ```
    #[must_use]
    pub fn abrange(&self, recurso: &Self) -> bool {
        self.empresa == recurso.empresa
            && Self::dimensao_abrange(self.filial, recurso.filial)
            && Self::dimensao_abrange(self.caixa, recurso.caixa)
            && Self::dimensao_abrange(self.centro_custo, recurso.centro_custo)
    }

    fn dimensao_abrange(autorizado: Option<Id>, recurso: Option<Id>) -> bool {
        match autorizado {
            None => true,
            Some(id) => recurso == Some(id),
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn empresas_diferentes_nunca_se_abrangem() {
        let a = Escopo::empresa_inteira(Id::novo());
        let b = Escopo::empresa_inteira(Id::novo());
        assert!(!a.abrange(&b));
    }

    #[test]
    fn vendedor_so_ve_a_propria_carteira() {
        let empresa = Id::novo();
        let cliente_da_carteira = Id::novo();
        let cliente_de_outro_vendedor = Id::novo();

        // Reaproveitamos "centro_custo" como um id genérico de carteira neste teste —
        // a regra de abrangência é idêntica em qualquer dimensão.
        let vendedor = Escopo::empresa_inteira(empresa).com_centro_custo(cliente_da_carteira);
        let recurso_proprio =
            Escopo::empresa_inteira(empresa).com_centro_custo(cliente_da_carteira);
        let recurso_alheio =
            Escopo::empresa_inteira(empresa).com_centro_custo(cliente_de_outro_vendedor);

        assert!(vendedor.abrange(&recurso_proprio));
        assert!(!vendedor.abrange(&recurso_alheio));
    }

    #[test]
    fn operador_so_opera_o_caixa_em_que_abriu_turno() {
        let empresa = Id::novo();
        let caixa_1 = Id::novo();
        let caixa_2 = Id::novo();

        let operador = Escopo::empresa_inteira(empresa).com_caixa(caixa_1);
        assert!(operador.abrange(&Escopo::empresa_inteira(empresa).com_caixa(caixa_1)));
        assert!(!operador.abrange(&Escopo::empresa_inteira(empresa).com_caixa(caixa_2)));
    }
}
