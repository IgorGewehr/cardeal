//! A ordem de serviço e sua máquina de estados.
//!
//! `docs/modulos/os.md` §3 e §4. Domínio puro: as transições só validam e mutam `self`;
//! quem chama peças/estoque/financeiro é o comando.

use cardeal_kernel::{Data, Dinheiro, Id, Versao};
use serde::{Deserialize, Serialize};

use crate::erros::ErroOs;

/// O estado de uma [`OrdemServico`] (`docs/modulos/os.md` §4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EstadoOs {
    /// Recém-aberta, sem laudo nem orçamento ainda.
    Aberta,
    /// Laudo técnico registrado.
    EmDiagnostico,
    /// Orçamento montado e enviado ao cliente.
    AguardandoAprovacao,
    /// O cliente aprovou o orçamento.
    Aprovada,
    /// O cliente recusou o orçamento. Terminal.
    Reprovada,
    /// Peças e mão de obra sendo aplicadas.
    EmExecucao,
    /// Execução concluída, pronta para faturar.
    Concluida,
    /// Faturada — gerou título a receber no financeiro. Terminal.
    Faturada,
    /// Cancelada antes de faturar. Terminal.
    Cancelada,
}

impl EstadoOs {
    /// O rótulo em português.
    #[must_use]
    pub const fn rotulo(self) -> &'static str {
        match self {
            Self::Aberta => "Aberta",
            Self::EmDiagnostico => "EmDiagnostico",
            Self::AguardandoAprovacao => "AguardandoAprovacao",
            Self::Aprovada => "Aprovada",
            Self::Reprovada => "Reprovada",
            Self::EmExecucao => "EmExecucao",
            Self::Concluida => "Concluida",
            Self::Faturada => "Faturada",
            Self::Cancelada => "Cancelada",
        }
    }

    /// Verdadeiro se o orçamento (peça/mão de obra) ainda pode ser editado.
    #[must_use]
    pub const fn aceita_edicao_de_orcamento(self) -> bool {
        matches!(self, Self::Aberta | Self::EmDiagnostico)
    }
}

/// Uma ordem de serviço — abertura, laudo, orçamento, execução e faturamento.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrdemServico {
    /// Identidade.
    pub id: Id,
    /// A empresa.
    pub empresa: Id,
    /// Sequencial por empresa.
    pub numero: u64,
    /// O cliente (papel `Cliente` em `clientes_pessoa`).
    pub cliente: Id,
    /// Descrição livre do equipamento ("Notebook Dell XPS 13").
    pub equipamento: String,
    /// Quando foi aberta.
    pub data_abertura: Data,
    /// O técnico responsável.
    pub tecnico_responsavel: Id,
    /// O estado atual.
    pub estado: EstadoOs,
    /// Nome/documento de quem aprovou o orçamento — a aprovação nunca é implícita
    /// (`docs/modulos/os.md` §11.2).
    pub aprovado_por: Option<String>,
    /// Prazo de garantia, em dias, sobre as peças aplicadas.
    pub garantia_dias: u16,
    /// A soma dos itens cobrados (peças não cobertas por garantia + mão de obra).
    pub valor_total: Dinheiro,
    /// Quantos itens (peça + mão de obra) o orçamento tem — separado de `valor_total` porque
    /// um reparo em garantia tem itens a custo zero para o cliente e ainda assim precisa
    /// seguir para aprovação e faturamento (`ErroOs::OrcamentoVazio` recusa "sem item nenhum",
    /// não "soma zero").
    pub itens_orcamento: u32,
    /// Versão para bloqueio otimista.
    pub versao: Versao,
}

impl OrdemServico {
    /// Abre uma ordem de serviço nova.
    ///
    /// # Errors
    /// [`ErroOs::EquipamentoVazio`].
    pub fn abrir(
        empresa: Id,
        numero: u64,
        cliente: Id,
        equipamento: impl Into<String>,
        tecnico_responsavel: Id,
        data_abertura: Data,
        garantia_dias: u16,
    ) -> Result<Self, ErroOs> {
        let equipamento = equipamento.into().trim().to_string();
        if equipamento.is_empty() {
            return Err(ErroOs::EquipamentoVazio);
        }
        Ok(Self {
            id: Id::novo(),
            empresa,
            numero,
            cliente,
            equipamento,
            data_abertura,
            tecnico_responsavel,
            estado: EstadoOs::Aberta,
            aprovado_por: None,
            garantia_dias,
            valor_total: Dinheiro::ZERO,
            itens_orcamento: 0,
            versao: Versao::INICIAL,
        })
    }

    fn transitar(&mut self, novo: EstadoOs) {
        self.estado = novo;
        self.versao = self.versao.proxima();
    }

    fn exigir_estado(&self, esperado: EstadoOs) -> Result<(), ErroOs> {
        if self.estado == esperado {
            Ok(())
        } else {
            Err(ErroOs::EstadoInvalido {
                atual: self.estado.rotulo(),
                esperado: esperado.rotulo(),
            })
        }
    }

    /// Registra que o laudo técnico foi feito: `Aberta` → `EmDiagnostico`. Chamável de novo
    /// enquanto `EmDiagnostico` para corrigir um laudo digitado errado — não transiciona de
    /// novo, só bump de versão (o comando sobrescreve o texto; a consulta já lê sempre o
    /// laudo mais recente por `ordem_servico`).
    ///
    /// # Errors
    /// [`ErroOs::EstadoInvalido`] fora de `Aberta`/`EmDiagnostico`.
    pub fn registrar_laudo(&mut self) -> Result<(), ErroOs> {
        match self.estado {
            EstadoOs::Aberta => {
                self.transitar(EstadoOs::EmDiagnostico);
                Ok(())
            }
            EstadoOs::EmDiagnostico => {
                self.versao = self.versao.proxima();
                Ok(())
            }
            _ => Err(ErroOs::EstadoInvalido {
                atual: self.estado.rotulo(),
                esperado: "Aberta ou EmDiagnostico",
            }),
        }
    }

    /// Acrescenta um item ao orçamento — chamado a cada item novo (fase de montagem,
    /// `Aberta`/`EmDiagnostico`) ou a cada mão de obra extra registrada durante a execução
    /// (`EmExecucao`).
    ///
    /// # Errors
    /// [`ErroOs::EstadoInvalido`] se o estado não aceita alterar o orçamento.
    pub fn adicionar_ao_orcamento(&mut self, total_do_item: Dinheiro) -> Result<(), ErroOs> {
        if !self.estado.aceita_edicao_de_orcamento() && self.estado != EstadoOs::EmExecucao {
            return Err(ErroOs::EstadoInvalido {
                atual: self.estado.rotulo(),
                esperado: "Aberta, EmDiagnostico ou EmExecucao",
            });
        }
        self.valor_total += total_do_item;
        self.itens_orcamento += 1;
        self.versao = self.versao.proxima();
        Ok(())
    }

    /// Remove um item do orçamento antes de enviar para aprovação — corrige um item
    /// digitado errado sem precisar cancelar a OS inteira. Só antes da aprovação
    /// (`Aberta`/`EmDiagnostico`); depois disso a correção é uma nova negociação com o
    /// cliente, não uma edição silenciosa.
    ///
    /// # Errors
    /// [`ErroOs::EstadoInvalido`] se o estado não aceita mais editar o orçamento.
    pub fn remover_do_orcamento(&mut self, total_do_item: Dinheiro) -> Result<(), ErroOs> {
        if !self.estado.aceita_edicao_de_orcamento() {
            return Err(ErroOs::EstadoInvalido {
                atual: self.estado.rotulo(),
                esperado: "Aberta ou EmDiagnostico",
            });
        }
        self.valor_total = (self.valor_total - total_do_item).nao_negativo();
        self.itens_orcamento = self.itens_orcamento.saturating_sub(1);
        self.versao = self.versao.proxima();
        Ok(())
    }

    /// Envia o orçamento para aprovação: `Aberta`/`EmDiagnostico` → `AguardandoAprovacao`.
    ///
    /// # Errors
    /// [`ErroOs::EstadoInvalido`], [`ErroOs::OrcamentoVazio`] se não há nenhum item
    /// orçado — um reparo em garantia (itens a custo zero) segue normalmente.
    pub fn enviar_para_aprovacao(&mut self) -> Result<(), ErroOs> {
        if !self.estado.aceita_edicao_de_orcamento() {
            return Err(ErroOs::EstadoInvalido {
                atual: self.estado.rotulo(),
                esperado: "Aberta ou EmDiagnostico",
            });
        }
        if self.itens_orcamento == 0 {
            return Err(ErroOs::OrcamentoVazio);
        }
        self.transitar(EstadoOs::AguardandoAprovacao);
        Ok(())
    }

    /// O cliente aprova: `AguardandoAprovacao` → `Aprovada`. `identificacao` é o nome (e
    /// documento, quando houver) de quem aprovou — a aprovação nunca é implícita.
    ///
    /// # Errors
    /// [`ErroOs::OrcamentoJaDecidido`] se não estiver aguardando decisão;
    /// [`ErroOs::AprovacaoSemIdentificacao`] se `identificacao` vier vazia.
    pub fn aprovar(&mut self, identificacao: impl Into<String>) -> Result<(), ErroOs> {
        if self.estado != EstadoOs::AguardandoAprovacao {
            return Err(ErroOs::OrcamentoJaDecidido);
        }
        let identificacao = identificacao.into().trim().to_string();
        if identificacao.is_empty() {
            return Err(ErroOs::AprovacaoSemIdentificacao);
        }
        self.aprovado_por = Some(identificacao);
        self.transitar(EstadoOs::Aprovada);
        Ok(())
    }

    /// O cliente reprova: `AguardandoAprovacao` → `Reprovada` (terminal).
    ///
    /// # Errors
    /// [`ErroOs::OrcamentoJaDecidido`] se não estiver aguardando decisão.
    pub fn reprovar(&mut self) -> Result<(), ErroOs> {
        if self.estado != EstadoOs::AguardandoAprovacao {
            return Err(ErroOs::OrcamentoJaDecidido);
        }
        self.transitar(EstadoOs::Reprovada);
        Ok(())
    }

    /// Inicia a execução: `Aprovada` → `EmExecucao`.
    ///
    /// # Errors
    /// [`ErroOs::OrcamentoNaoAprovado`] se o estado não for `Aprovada`.
    pub fn iniciar_execucao(&mut self) -> Result<(), ErroOs> {
        if self.estado != EstadoOs::Aprovada {
            return Err(ErroOs::OrcamentoNaoAprovado);
        }
        self.transitar(EstadoOs::EmExecucao);
        Ok(())
    }

    /// Exige `EmExecucao` — usado por `AplicarPeca`/`RegistrarMaoDeObra`.
    ///
    /// # Errors
    /// [`ErroOs::EstadoInvalido`].
    pub fn exigir_em_execucao(&self) -> Result<(), ErroOs> {
        self.exigir_estado(EstadoOs::EmExecucao)
    }

    /// Conclui a execução: `EmExecucao` → `Concluida`.
    ///
    /// # Errors
    /// [`ErroOs::EstadoInvalido`].
    pub fn concluir_execucao(&mut self) -> Result<(), ErroOs> {
        self.exigir_estado(EstadoOs::EmExecucao)?;
        self.transitar(EstadoOs::Concluida);
        Ok(())
    }

    /// Fatura: `Concluida` → `Faturada` (terminal).
    ///
    /// # Errors
    /// [`ErroOs::OsJaFaturada`], [`ErroOs::OsNaoConcluida`].
    pub fn faturar(&mut self) -> Result<(), ErroOs> {
        if self.estado == EstadoOs::Faturada {
            return Err(ErroOs::OsJaFaturada);
        }
        if self.estado != EstadoOs::Concluida {
            return Err(ErroOs::OsNaoConcluida);
        }
        self.transitar(EstadoOs::Faturada);
        Ok(())
    }

    /// Cancela a ordem antes de faturar — nunca depois (`docs/modulos/os.md` §4).
    ///
    /// # Errors
    /// [`ErroOs::EstadoInvalido`] se já `Concluida`/`Faturada`/`Cancelada`/`Reprovada`.
    pub fn cancelar(&mut self) -> Result<(), ErroOs> {
        if !matches!(
            self.estado,
            EstadoOs::Aberta | EstadoOs::EmDiagnostico | EstadoOs::AguardandoAprovacao
        ) {
            return Err(ErroOs::EstadoInvalido {
                atual: self.estado.rotulo(),
                esperado: "Aberta, EmDiagnostico ou AguardandoAprovacao",
            });
        }
        self.transitar(EstadoOs::Cancelada);
        Ok(())
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn hoje() -> Data {
        Data::de_dias(20_000)
    }

    fn os_aberta() -> OrdemServico {
        OrdemServico::abrir(
            Id::novo(),
            1,
            Id::novo(),
            "Notebook Dell XPS 13",
            Id::novo(),
            hoje(),
            90,
        )
        .unwrap()
    }

    #[test]
    fn equipamento_vazio_e_recusado() {
        let erro = OrdemServico::abrir(Id::novo(), 1, Id::novo(), "   ", Id::novo(), hoje(), 90)
            .unwrap_err();
        assert_eq!(erro, ErroOs::EquipamentoVazio);
    }

    #[test]
    fn fluxo_feliz_completo() {
        let mut os = os_aberta();
        os.registrar_laudo().unwrap();
        assert_eq!(os.estado, EstadoOs::EmDiagnostico);

        os.adicionar_ao_orcamento(Dinheiro::reais(400)).unwrap();
        os.enviar_para_aprovacao().unwrap();
        assert_eq!(os.estado, EstadoOs::AguardandoAprovacao);

        os.aprovar("João Silva - CPF 529.982.247-25").unwrap();
        assert_eq!(os.estado, EstadoOs::Aprovada);

        os.iniciar_execucao().unwrap();
        assert_eq!(os.estado, EstadoOs::EmExecucao);

        os.concluir_execucao().unwrap();
        assert_eq!(os.estado, EstadoOs::Concluida);

        os.faturar().unwrap();
        assert_eq!(os.estado, EstadoOs::Faturada);
        assert_eq!(os.faturar().unwrap_err(), ErroOs::OsJaFaturada);
    }

    #[test]
    fn enviar_para_aprovacao_com_orcamento_vazio_e_recusado() {
        let mut os = os_aberta();
        assert_eq!(
            os.enviar_para_aprovacao().unwrap_err(),
            ErroOs::OrcamentoVazio
        );
    }

    #[test]
    fn iniciar_execucao_sem_aprovacao_e_recusado() {
        let mut os = os_aberta();
        assert_eq!(
            os.iniciar_execucao().unwrap_err(),
            ErroOs::OrcamentoNaoAprovado
        );
    }

    #[test]
    fn faturar_antes_de_concluir_e_recusado() {
        let mut os = os_aberta();
        assert_eq!(os.faturar().unwrap_err(), ErroOs::OsNaoConcluida);
    }

    #[test]
    fn cancelar_depois_de_concluida_e_recusado() {
        let mut os = os_aberta();
        os.adicionar_ao_orcamento(Dinheiro::reais(100)).unwrap();
        os.enviar_para_aprovacao().unwrap();
        os.aprovar("João Silva - CPF 529.982.247-25").unwrap();
        os.iniciar_execucao().unwrap();
        os.concluir_execucao().unwrap();
        assert!(os.cancelar().is_err());
    }

    #[test]
    fn reprovar_e_terminal() {
        let mut os = os_aberta();
        os.adicionar_ao_orcamento(Dinheiro::reais(100)).unwrap();
        os.enviar_para_aprovacao().unwrap();
        os.reprovar().unwrap();
        assert_eq!(os.estado, EstadoOs::Reprovada);
        assert_eq!(
            os.aprovar("Alguém").unwrap_err(),
            ErroOs::OrcamentoJaDecidido
        );
    }
}
