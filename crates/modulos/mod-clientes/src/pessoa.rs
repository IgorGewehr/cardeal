//! Pessoa e seus papéis — o cadastro único do Cardeal.
//!
//! `docs/modulos/clientes.md` §3 e §4. Cliente, fornecedor, transportadora, funcionário,
//! sócio e vendedor são **papéis da mesma pessoa** (regra crítica §11.2), nunca cadastros
//! separados. Domínio puro.

use cardeal_kernel::{Data, Id, Instante, Versao};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use crate::erros::ErroClientes;

/// Pessoa física ou jurídica.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TipoPessoa {
    /// Pessoa natural (CPF).
    Fisica,
    /// Pessoa jurídica (CNPJ).
    Juridica,
}

/// Um papel que uma pessoa exerce perante a empresa. Coexistem — a mesma `Pessoa` pode ser
/// `Cliente` e `Fornecedor` ao mesmo tempo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Papel {
    /// Compra da empresa.
    Cliente,
    /// Vende para a empresa.
    Fornecedor,
    /// Transporta mercadoria.
    Transportadora,
    /// Presta serviço interno (identidade só; folha não é deste módulo).
    Funcionario,
    /// Sócio da empresa.
    Socio,
    /// Vendedor com carteira.
    Vendedor,
}

/// O estado de uma [`Pessoa`] (`docs/modulos/clientes.md` §4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EstadoPessoa {
    /// Aparece na busca padrão; aceita edição.
    Ativa,
    /// Fora da busca padrão; nada foi apagado; pode reativar.
    Inativa,
    /// Nome e documento substituídos por identificadores anônimos (LGPD). Terminal.
    Anonimizada,
}

/// O cadastro de uma pessoa.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pessoa {
    /// Identidade.
    pub id: Id,
    /// A empresa dona do cadastro.
    pub empresa: Id,
    /// Física ou jurídica.
    pub tipo: TipoPessoa,
    /// Nome civil ou razão social.
    pub nome: String,
    /// Nome fantasia (só `Juridica`).
    pub nome_fantasia: Option<String>,
    /// Nascimento (PF) ou abertura (PJ).
    pub data_nascimento_abertura: Option<Data>,
    /// O estado.
    pub estado: EstadoPessoa,
    /// Quando foi anonimizada, se foi.
    pub anonimizado_em: Option<Instante>,
    /// Observação livre.
    pub observacao: Option<String>,
    /// Os papéis ativos e inativos.
    pub papeis: SmallVec<[PapelPessoa; 2]>,
    /// Versão para bloqueio otimista.
    pub versao: Versao,
    /// Quando foi criada.
    pub criado_em: Instante,
}

/// Um papel atribuído a uma pessoa, com sua própria vigência.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PapelPessoa {
    /// O papel.
    pub papel: Papel,
    /// Desde quando vale.
    pub ativo_desde: Data,
    /// Se o papel está ativo (pode ser desativado sem desativar a pessoa).
    pub ativo: bool,
}

impl Pessoa {
    /// Verdadeiro se a pessoa tem o papel dado, ativo.
    #[must_use]
    pub fn tem_papel(&self, papel: Papel) -> bool {
        self.papeis.iter().any(|p| p.papel == papel && p.ativo)
    }

    /// Verdadeiro se aceita alteração cadastral (não foi anonimizada).
    #[must_use]
    pub fn editavel(&self) -> bool {
        self.estado != EstadoPessoa::Anonimizada
    }

    /// Acrescenta um papel. Reativa um papel antes desativado, em vez de duplicar.
    ///
    /// # Errors
    /// [`ErroClientes::PessoaAnonimizada`] se já anonimizada;
    /// [`ErroClientes::PapelJaExiste`] se o papel já está ativo.
    pub fn adicionar_papel(&mut self, papel: Papel, quando: Data) -> Result<(), ErroClientes> {
        if !self.editavel() {
            return Err(ErroClientes::PessoaAnonimizada);
        }
        if let Some(existente) = self.papeis.iter_mut().find(|p| p.papel == papel) {
            if existente.ativo {
                return Err(ErroClientes::PapelJaExiste(papel));
            }
            existente.ativo = true;
            existente.ativo_desde = quando;
        } else {
            self.papeis.push(PapelPessoa {
                papel,
                ativo_desde: quando,
                ativo: true,
            });
        }
        self.versao = self.versao.proxima();
        Ok(())
    }

    /// Desativa um papel. `tem_pendencia` é apurado pelo comando (títulos em aberto naquele
    /// papel) — o domínio só aplica a regra (`docs/modulos/clientes.md` §11.7).
    ///
    /// Remover um papel ausente ou já inativo é um no-op bem-sucedido.
    ///
    /// # Errors
    /// [`ErroClientes::PapelComPendencia`] se `tem_pendencia`.
    pub fn remover_papel(&mut self, papel: Papel, tem_pendencia: bool) -> Result<(), ErroClientes> {
        if tem_pendencia {
            return Err(ErroClientes::PapelComPendencia(papel));
        }
        if let Some(p) = self.papeis.iter_mut().find(|p| p.papel == papel && p.ativo) {
            p.ativo = false;
            self.versao = self.versao.proxima();
        }
        Ok(())
    }

    /// Desativa a pessoa (sem uso recente). Reversível por [`Self::reativar`].
    pub fn desativar(&mut self) {
        if self.estado == EstadoPessoa::Ativa {
            self.estado = EstadoPessoa::Inativa;
            self.versao = self.versao.proxima();
        }
    }

    /// Reativa uma pessoa inativa.
    pub fn reativar(&mut self) {
        if self.estado == EstadoPessoa::Inativa {
            self.estado = EstadoPessoa::Ativa;
            self.versao = self.versao.proxima();
        }
    }

    /// Anonimiza a pessoa (LGPD): substitui nome, fantasia, observação e datas pessoais por
    /// identificadores anônimos e marca o estado como terminal. **Não toca no Razão** —
    /// títulos e lançamentos ligados a `self.id` permanecem (`docs/modulos/clientes.md`
    /// §11.6). O comando é quem apaga endereços/contatos/documentos.
    ///
    /// # Errors
    /// [`ErroClientes::PessoaAnonimizada`] se já estava anonimizada.
    pub fn anonimizar(&mut self, agora: Instante) -> Result<(), ErroClientes> {
        if self.estado == EstadoPessoa::Anonimizada {
            return Err(ErroClientes::PessoaAnonimizada);
        }
        self.nome = format!("Titular anonimizado {}", self.id.curto());
        self.nome_fantasia = None;
        self.observacao = None;
        self.data_nascimento_abertura = None;
        self.estado = EstadoPessoa::Anonimizada;
        self.anonimizado_em = Some(agora);
        self.versao = self.versao.proxima();
        Ok(())
    }
}

/// Monta uma [`Pessoa`] nova com um papel inicial.
#[derive(Debug, Clone)]
pub struct ConstrutorPessoa {
    empresa: Id,
    tipo: TipoPessoa,
    nome: String,
    nome_fantasia: Option<String>,
    data_nascimento_abertura: Option<Data>,
    observacao: Option<String>,
    papel_inicial: Papel,
    agora: Instante,
    hoje: Data,
}

impl ConstrutorPessoa {
    /// Começa uma pessoa. `papel_inicial` é o primeiro papel (ex.: `Cliente`).
    #[must_use]
    pub fn nova(
        empresa: Id,
        tipo: TipoPessoa,
        nome: impl Into<String>,
        papel_inicial: Papel,
        agora: Instante,
        hoje: Data,
    ) -> Self {
        Self {
            empresa,
            tipo,
            nome: nome.into(),
            nome_fantasia: None,
            data_nascimento_abertura: None,
            observacao: None,
            papel_inicial,
            agora,
            hoje,
        }
    }

    /// Nome fantasia (só válido para `Juridica`).
    #[must_use]
    pub fn nome_fantasia(mut self, f: impl Into<String>) -> Self {
        self.nome_fantasia = Some(f.into());
        self
    }

    /// Data de nascimento (PF) ou abertura (PJ).
    #[must_use]
    pub const fn nascimento_abertura(mut self, d: Data) -> Self {
        self.data_nascimento_abertura = Some(d);
        self
    }

    /// Observação livre.
    #[must_use]
    pub fn observacao(mut self, o: impl Into<String>) -> Self {
        self.observacao = Some(o.into());
        self
    }

    /// Valida e constrói.
    ///
    /// # Errors
    /// [`ErroClientes::NomeVazio`], [`ErroClientes::FantasiaEmPessoaFisica`].
    pub fn construir(self) -> Result<Pessoa, ErroClientes> {
        let nome = self.nome.trim().to_string();
        if nome.is_empty() {
            return Err(ErroClientes::NomeVazio);
        }
        if self.nome_fantasia.is_some() && self.tipo == TipoPessoa::Fisica {
            return Err(ErroClientes::FantasiaEmPessoaFisica);
        }
        Ok(Pessoa {
            id: Id::novo(),
            empresa: self.empresa,
            tipo: self.tipo,
            nome,
            nome_fantasia: self.nome_fantasia.map(|f| f.trim().to_string()),
            data_nascimento_abertura: self.data_nascimento_abertura,
            estado: EstadoPessoa::Ativa,
            anonimizado_em: None,
            observacao: self.observacao,
            papeis: smallvec::smallvec![PapelPessoa {
                papel: self.papel_inicial,
                ativo_desde: self.hoje,
                ativo: true,
            }],
            versao: Versao::INICIAL,
            criado_em: self.agora,
        })
    }
}

#[cfg(test)]
mod testes {
    use cardeal_kernel::Fuso;

    use super::*;

    fn hoje() -> Data {
        Data::hoje(Fuso::BRASILIA)
    }

    fn pessoa(tipo: TipoPessoa) -> Pessoa {
        ConstrutorPessoa::nova(
            Id::novo(),
            tipo,
            "Mercado Bom Preço LTDA",
            Papel::Cliente,
            Instante::agora(),
            hoje(),
        )
        .construir()
        .unwrap()
    }

    #[test]
    fn nasce_ativa_com_o_papel_inicial() {
        let p = pessoa(TipoPessoa::Juridica);
        assert_eq!(p.estado, EstadoPessoa::Ativa);
        assert!(p.tem_papel(Papel::Cliente));
        assert!(!p.tem_papel(Papel::Fornecedor));
    }

    #[test]
    fn nome_vazio_e_fantasia_em_pf_sao_recusados() {
        let erro = ConstrutorPessoa::nova(
            Id::novo(),
            TipoPessoa::Fisica,
            "   ",
            Papel::Cliente,
            Instante::agora(),
            hoje(),
        )
        .construir()
        .unwrap_err();
        assert_eq!(erro, ErroClientes::NomeVazio);

        let erro = ConstrutorPessoa::nova(
            Id::novo(),
            TipoPessoa::Fisica,
            "João",
            Papel::Cliente,
            Instante::agora(),
            hoje(),
        )
        .nome_fantasia("Jota")
        .construir()
        .unwrap_err();
        assert_eq!(erro, ErroClientes::FantasiaEmPessoaFisica);
    }

    #[test]
    fn papel_coexiste_e_nao_duplica() {
        let mut p = pessoa(TipoPessoa::Juridica);
        p.adicionar_papel(Papel::Fornecedor, hoje()).unwrap();
        assert!(p.tem_papel(Papel::Cliente) && p.tem_papel(Papel::Fornecedor));

        let erro = p.adicionar_papel(Papel::Fornecedor, hoje()).unwrap_err();
        assert_eq!(erro, ErroClientes::PapelJaExiste(Papel::Fornecedor));

        // Remover e re-adicionar reativa o mesmo registro.
        p.remover_papel(Papel::Fornecedor, false).unwrap();
        assert!(!p.tem_papel(Papel::Fornecedor));
        p.adicionar_papel(Papel::Fornecedor, hoje()).unwrap();
        assert_eq!(
            p.papeis
                .iter()
                .filter(|x| x.papel == Papel::Fornecedor)
                .count(),
            1
        );
    }

    #[test]
    fn remover_papel_com_pendencia_falha() {
        let mut p = pessoa(TipoPessoa::Juridica);
        let erro = p.remover_papel(Papel::Cliente, true).unwrap_err();
        assert_eq!(erro, ErroClientes::PapelComPendencia(Papel::Cliente));
        assert!(p.tem_papel(Papel::Cliente));
    }

    #[test]
    fn anonimizar_e_terminal_e_preserva_o_id() {
        let mut p = pessoa(TipoPessoa::Juridica);
        let id = p.id;
        p.observacao = Some("cliente VIP".to_string());
        p.anonimizar(Instante::agora()).unwrap();

        assert_eq!(p.estado, EstadoPessoa::Anonimizada);
        assert_eq!(p.id, id);
        assert!(p.observacao.is_none());
        assert!(p.nome.starts_with("Titular anonimizado"));
        assert!(!p.editavel());

        assert_eq!(
            p.anonimizar(Instante::agora()).unwrap_err(),
            ErroClientes::PessoaAnonimizada
        );
        assert_eq!(
            p.adicionar_papel(Papel::Fornecedor, hoje()).unwrap_err(),
            ErroClientes::PessoaAnonimizada
        );
    }

    #[test]
    fn desativar_e_reativar() {
        let mut p = pessoa(TipoPessoa::Fisica);
        p.desativar();
        assert_eq!(p.estado, EstadoPessoa::Inativa);
        p.reativar();
        assert_eq!(p.estado, EstadoPessoa::Ativa);
    }
}
