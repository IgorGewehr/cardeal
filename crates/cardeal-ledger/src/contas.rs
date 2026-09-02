//! Resolução de papel semântico e código para o `Id` real da conta.
//!
//! Todo módulo que posta no Razão pede `contas.papel(PapelConta::Cmv)` em vez de conhecer
//! o código `"5.1"` — é o que permite o mesmo código de módulo funcionar em qualquer tenant,
//! mesmo que o plano de contas tenha sido customizado. Ver
//! `docs/contratos-internos.md` §6, regra 4.

use cardeal_kernel::Id;

use crate::conta::PapelConta;
use crate::erros::ErroRazao;
use crate::porta::PortaRazao;

/// Resolve contas de uma empresa por papel ou por código.
///
/// Empresta a porta em vez de copiar seus dados — em `cardeal-storage`, a implementação de
/// [`PortaRazao`] provavelmente cacheia os dois mapas (`papel → Id`, `código → Id`) na
/// abertura da conexão, então este tipo é só uma fachada fina sobre esse cache.
pub struct Contas<'a, P: PortaRazao> {
    porta: &'a P,
    empresa: Id,
}

impl<'a, P: PortaRazao> Contas<'a, P> {
    /// Cria o resolvedor para a empresa dada.
    #[must_use]
    pub const fn nova(porta: &'a P, empresa: Id) -> Self {
        Self { porta, empresa }
    }

    /// A conta analítica que exerce este papel no plano da empresa.
    ///
    /// # Errors
    /// [`ErroRazao::PapelNaoMapeado`] se nenhuma conta do plano tiver esse papel — sinal de
    /// que o plano de contas foi editado sem cuidado, ou que a empresa ainda não tem o
    /// plano padrão semeado.
    pub fn papel(&self, papel: PapelConta) -> Result<Id, ErroRazao> {
        self.porta
            .conta_por_papel(self.empresa, papel)
            .ok_or(ErroRazao::PapelNaoMapeado(papel))
    }

    /// A conta com este código exato no plano da empresa.
    ///
    /// # Errors
    /// [`ErroRazao::CodigoNaoEncontrado`] se não existir conta com esse código.
    pub fn por_codigo(&self, codigo: &str) -> Result<Id, ErroRazao> {
        self.porta
            .conta_por_codigo(self.empresa, codigo)
            .ok_or_else(|| ErroRazao::CodigoNaoEncontrado(codigo.to_string()))
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::testkit_interno::RazaoEmMemoria;

    #[test]
    fn resolve_por_papel_e_por_codigo() {
        let mut mundo = RazaoEmMemoria::nova();
        let empresa = Id::novo();
        let caixa = mundo.semear_plano_padrao(empresa);

        let contas = Contas::nova(&mundo, empresa);
        assert_eq!(contas.papel(PapelConta::Caixa).unwrap(), caixa);
        assert_eq!(contas.por_codigo("1.1.01").unwrap(), caixa);
        assert!(contas.por_codigo("9.9.99").is_err());
    }

    #[test]
    fn papel_sem_conta_mapeada_da_erro_claro() {
        let mundo = RazaoEmMemoria::nova();
        let contas = Contas::nova(&mundo, Id::novo());
        let erro = contas.papel(PapelConta::ReceitaHospedagem).unwrap_err();
        assert!(matches!(
            erro,
            ErroRazao::PapelNaoMapeado(PapelConta::ReceitaHospedagem)
        ));
    }
}
