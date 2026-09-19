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

    /// Verdadeiro se o orçamento (peça/mão de obra) ainda pode ser editado **antes de
    /// enviar para aprovação** — não inclui `Concluida`: usado só por
    /// [`OrdemServico::enviar_para_aprovacao`]. `adicionar_ao_orcamento`/
    /// `remover_do_orcamento` têm sua própria checagem, mais ampla, que também aceita
    /// `Concluida` (ver [`OrdemServico::aceita_ajuste_de_itens`]).
    #[must_use]
    pub const fn aceita_edicao_de_orcamento(self) -> bool {
        matches!(self, Self::Aberta | Self::EmDiagnostico)
    }

    /// Verdadeiro se um item de peça/mão de obra pode ser adicionado ou removido do
    /// orçamento. Mais amplo que [`Self::aceita_edicao_de_orcamento`]: inclui `Concluida`
    /// — uma OS concluída (pronta para faturar) mas ainda não faturada, ou desfaturada de
    /// volta a este estado por [`OrdemServico::desfaturar`], ainda é rascunho de
    /// valores/serviços, não histórico fechado. Não reabre o trâmite de aprovação por si
    /// só — só quem chama decide se reenvia para aprovação depois de ajustar.
    #[must_use]
    pub const fn aceita_ajuste_de_itens(self) -> bool {
        matches!(self, Self::Aberta | Self::EmDiagnostico | Self::Concluida)
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
    /// Descrição livre do aparelho trazido para reparo ("Notebook Dell XPS 13") —
    /// **obrigatória** na abertura: o balcão relatou confundir o antigo rótulo
    /// "Equipamento (opcional)" com "equipamento usado no reparo" e abrir OS sem saber de
    /// qual aparelho se tratava. Ainda pode ser corrigida depois via
    /// [`OrdemServico::completar_equipamento`]/`EditarDadosDaOrdem`.
    pub equipamento: String,
    /// O que o cliente relatou querer resolver, capturado na recepção — junto do nome do
    /// cliente e do aparelho, texto obrigatório para abrir uma OS. **Não** é o
    /// parecer/diagnóstico técnico — isso é [`crate::laudo::LaudoTecnico`], escrito pelo
    /// técnico depois, em outro momento do fluxo.
    pub defeito_relatado: String,
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
    /// Abre uma ordem de serviço nova. `equipamento` (o aparelho trazido para reparo) e
    /// `defeito_relatado` são os dois textos obrigatórios além do cliente — o balcão nunca
    /// deve abrir OS sem saber de qual aparelho se trata.
    ///
    /// # Errors
    /// [`ErroOs::DefeitoRelatadoVazio`], [`ErroOs::EquipamentoVazio`].
    // Construtor de domínio: cada argumento é um dado obrigatório e distinto da abertura de
    // uma OS; agrupá-los num struct só moveria a mesma lista para outro lugar.
    #[allow(clippy::too_many_arguments)]
    pub fn abrir(
        empresa: Id,
        numero: u64,
        cliente: Id,
        equipamento: impl Into<String>,
        defeito_relatado: impl Into<String>,
        tecnico_responsavel: Id,
        data_abertura: Data,
        garantia_dias: u16,
    ) -> Result<Self, ErroOs> {
        let equipamento = equipamento.into().trim().to_string();
        let defeito_relatado = defeito_relatado.into().trim().to_string();
        if equipamento.is_empty() {
            return Err(ErroOs::EquipamentoVazio);
        }
        if defeito_relatado.is_empty() {
            return Err(ErroOs::DefeitoRelatadoVazio);
        }
        Ok(Self {
            id: Id::novo(),
            empresa,
            numero,
            cliente,
            equipamento,
            defeito_relatado,
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

    /// Verdadeiro se a OS ainda aceita edição de dados gerais (equipamento/defeito
    /// relatado) — qualquer estado não-terminal. Uma OS finalizada é histórico, não
    /// rascunho.
    #[must_use]
    pub const fn aceita_edicao_de_dados(&self) -> bool {
        !matches!(
            self.estado,
            EstadoOs::Faturada | EstadoOs::Cancelada | EstadoOs::Reprovada
        )
    }

    fn exigir_nao_finalizada(&self) -> Result<(), ErroOs> {
        if self.aceita_edicao_de_dados() {
            Ok(())
        } else {
            Err(ErroOs::OrdemFinalizada)
        }
    }

    /// Corrige ou completa a descrição do equipamento/aparelho depois da abertura (erro de
    /// digitação, detalhe que faltou).
    ///
    /// # Errors
    /// [`ErroOs::OrdemFinalizada`] se a OS já estiver `Faturada`/`Cancelada`/`Reprovada`;
    /// [`ErroOs::EquipamentoVazio`] se `equipamento` vier vazio — obrigatório desde a
    /// abertura, uma correção não pode apagá-lo.
    pub fn completar_equipamento(&mut self, equipamento: impl Into<String>) -> Result<(), ErroOs> {
        self.exigir_nao_finalizada()?;
        let equipamento = equipamento.into().trim().to_string();
        if equipamento.is_empty() {
            return Err(ErroOs::EquipamentoVazio);
        }
        self.equipamento = equipamento;
        self.versao = self.versao.proxima();
        Ok(())
    }

    /// Acrescenta um complemento ao defeito relatado — para quando o cliente lembra de mais
    /// detalhe depois da abertura. **Nunca sobrescreve** o relato original; some a ele.
    ///
    /// # Errors
    /// [`ErroOs::OrdemFinalizada`]; [`ErroOs::DefeitoRelatadoVazio`] se o complemento vier
    /// vazio.
    pub fn complementar_defeito_relatado(
        &mut self,
        complemento: impl Into<String>,
    ) -> Result<(), ErroOs> {
        self.exigir_nao_finalizada()?;
        let complemento = complemento.into().trim().to_string();
        if complemento.is_empty() {
            return Err(ErroOs::DefeitoRelatadoVazio);
        }
        self.defeito_relatado = format!("{} | {}", self.defeito_relatado, complemento);
        self.versao = self.versao.proxima();
        Ok(())
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
    /// `Aberta`/`EmDiagnostico`), a cada mão de obra extra registrada durante a execução
    /// (`EmExecucao`), ou por correção depois de concluída (`Concluida`, inclusive uma OS
    /// desfaturada de volta a este estado — ver [`OrdemServico::desfaturar`]).
    ///
    /// # Errors
    /// [`ErroOs::EstadoInvalido`] se o estado não aceita alterar o orçamento.
    pub fn adicionar_ao_orcamento(&mut self, total_do_item: Dinheiro) -> Result<(), ErroOs> {
        if !self.estado.aceita_ajuste_de_itens() && self.estado != EstadoOs::EmExecucao {
            return Err(ErroOs::EstadoInvalido {
                atual: self.estado.rotulo(),
                esperado: "Aberta, EmDiagnostico, EmExecucao ou Concluida",
            });
        }
        self.valor_total += total_do_item;
        self.itens_orcamento += 1;
        self.versao = self.versao.proxima();
        Ok(())
    }

    /// Remove um item do orçamento — corrige um item digitado errado sem precisar cancelar
    /// a OS inteira. Aceita `Aberta`/`EmDiagnostico` (antes de enviar para aprovação) e
    /// também `Concluida` (correção pós-execução, inclusive numa OS desfaturada de volta a
    /// este estado — ver [`OrdemServico::desfaturar`]); fora disso a correção é uma nova
    /// negociação com o cliente, não uma edição silenciosa.
    ///
    /// # Errors
    /// [`ErroOs::EstadoInvalido`] se o estado não aceita mais editar o orçamento.
    pub fn remover_do_orcamento(&mut self, total_do_item: Dinheiro) -> Result<(), ErroOs> {
        if !self.estado.aceita_ajuste_de_itens() {
            return Err(ErroOs::EstadoInvalido {
                atual: self.estado.rotulo(),
                esperado: "Aberta, EmDiagnostico ou Concluida",
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

    /// Fatura: de **qualquer estado não-terminal** → `Faturada` (terminal), não só
    /// `Concluida`.
    ///
    /// Pedido explícito do usuário (2026-09-15): laudo → orçamento → aprovação → execução →
    /// conclusão são passos **opcionais** do trâmite técnico, não um portão que bloqueia o
    /// faturamento — o balcão precisa poder faturar assim que a OS abre, quando o serviço não
    /// precisa de todo o trâmite formal (um conserto rápido, cobrado na hora). `valor_total`
    /// já soma no momento em que o item entra no orçamento (`Self::adicionar_ao_orcamento`),
    /// não quando é `AplicarPeca`-do — então faturar sem nunca passar por `EmExecucao` cobra
    /// do cliente normalmente, mas **não** deduz peça nenhuma do estoque
    /// ([`crate::execucao::ItemPeca::total_custo`] só conta custo de peça `aplicada`, então o
    /// CMV desse item fica zero). Quem pula a execução é responsável por aplicar a peça à
    /// parte (`AplicarPeca` continua disponível em `EmExecucao`) se quiser o estoque
    /// correto — o faturamento em si nunca é bloqueado por isso.
    ///
    /// # Errors
    /// [`ErroOs::OsJaFaturada`] se já `Faturada`; [`ErroOs::OsCanceladaOuReprovada`] se
    /// `Cancelada`/`Reprovada` — essas nunca faturam.
    pub fn faturar(&mut self) -> Result<(), ErroOs> {
        if self.estado == EstadoOs::Faturada {
            return Err(ErroOs::OsJaFaturada);
        }
        if matches!(self.estado, EstadoOs::Cancelada | EstadoOs::Reprovada) {
            return Err(ErroOs::OsCanceladaOuReprovada);
        }
        self.transitar(EstadoOs::Faturada);
        Ok(())
    }

    /// Cancela a ordem antes de faturar — nunca depois (`docs/modulos/os.md` §4 e §11 regra
    /// 5). Aceita cancelar também a partir de `Aprovada`/`EmExecucao` — o cliente pode desistir
    /// depois de aprovar (peça indisponível, mudou de ideia), e o técnico pode precisar
    /// abortar em plena execução (peça quebrou, cliente cancelou no meio do conserto); quando
    /// já há peça aplicada, quem chama este método (`CancelarOrdemServico`) é responsável por
    /// estornar cada [`crate::execucao::ItemPeca`] aplicado de volta ao estoque — nunca fica
    /// consumo órfão sem contrapartida.
    ///
    /// # Errors
    /// [`ErroOs::EstadoInvalido`] se já `Concluida`/`Faturada`/`Cancelada`/`Reprovada`.
    pub fn cancelar(&mut self) -> Result<(), ErroOs> {
        if !matches!(
            self.estado,
            EstadoOs::Aberta
                | EstadoOs::EmDiagnostico
                | EstadoOs::AguardandoAprovacao
                | EstadoOs::Aprovada
                | EstadoOs::EmExecucao
        ) {
            return Err(ErroOs::EstadoInvalido {
                atual: self.estado.rotulo(),
                esperado: "Aberta, EmDiagnostico, AguardandoAprovacao, Aprovada ou EmExecucao",
            });
        }
        self.transitar(EstadoOs::Cancelada);
        Ok(())
    }

    /// Desfatura: `Faturada` → `Concluida`. Devolve a OS a um estado que aceita ajuste de
    /// itens/valores ([`Self::aceita_ajuste_de_itens`]) sem repetir o trâmite técnico
    /// inteiro (laudo/aprovação/execução) — `faturar` aceita faturar de qualquer estado
    /// não-terminal, então `Concluida` é o pouso genérico de "pronta para faturar de novo",
    /// independente de qual estado ela tinha antes de faturar. Quem chama
    /// (`DesfaturarOrdemServico`) é responsável por reverter o título/lançamento no
    /// financeiro **antes** de chamar isto — o domínio só valida e muda o próprio estado.
    ///
    /// # Errors
    /// [`ErroOs::OsNaoFaturada`] se o estado não for `Faturada`.
    pub fn desfaturar(&mut self) -> Result<(), ErroOs> {
        if self.estado != EstadoOs::Faturada {
            return Err(ErroOs::OsNaoFaturada);
        }
        self.transitar(EstadoOs::Concluida);
        Ok(())
    }

    /// Reabre uma ordem cancelada: `Cancelada` → `Aberta`. Uma OS cancelada nunca chegou a
    /// ser faturada (`cancelar` está bloqueado depois de `Concluida`/`Faturada`), então não
    /// há financeiro para reverter aqui — laudo/orçamento/itens já gravados continuam
    /// intactos, só o estado destrava de novo.
    ///
    /// # Errors
    /// [`ErroOs::OsNaoCancelada`] se o estado não for `Cancelada`.
    pub fn reabrir(&mut self) -> Result<(), ErroOs> {
        if self.estado != EstadoOs::Cancelada {
            return Err(ErroOs::OsNaoCancelada);
        }
        self.transitar(EstadoOs::Aberta);
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
            "Não liga",
            Id::novo(),
            hoje(),
            90,
        )
        .unwrap()
    }

    #[test]
    fn defeito_relatado_vazio_e_recusado() {
        let erro = OrdemServico::abrir(
            Id::novo(),
            1,
            Id::novo(),
            "Notebook Dell XPS 13",
            "   ",
            Id::novo(),
            hoje(),
            90,
        )
        .unwrap_err();
        assert_eq!(erro, ErroOs::DefeitoRelatadoVazio);
    }

    #[test]
    fn equipamento_vazio_e_recusado() {
        // O aparelho trazido para reparo é obrigatório na abertura — o balcão relatou
        // confundir o antigo campo opcional com "equipamento usado no reparo" e abrir OS
        // sem registrar de qual aparelho se tratava.
        let erro = OrdemServico::abrir(
            Id::novo(),
            1,
            Id::novo(),
            "   ",
            "Não liga",
            Id::novo(),
            hoje(),
            90,
        )
        .unwrap_err();
        assert_eq!(erro, ErroOs::EquipamentoVazio);
    }

    #[test]
    fn completar_equipamento_e_complementar_defeito_relatado() {
        let mut os = os_aberta();
        os.completar_equipamento("Notebook Dell XPS 13 - N7548")
            .unwrap();
        assert_eq!(os.equipamento, "Notebook Dell XPS 13 - N7548");

        os.complementar_defeito_relatado("Também não carrega a bateria")
            .unwrap();
        assert_eq!(
            os.defeito_relatado,
            "Não liga | Também não carrega a bateria"
        );

        assert_eq!(
            os.complementar_defeito_relatado("   ").unwrap_err(),
            ErroOs::DefeitoRelatadoVazio
        );
    }

    #[test]
    fn editar_dados_e_recusado_apos_finalizar() {
        let mut os = os_aberta();
        os.adicionar_ao_orcamento(Dinheiro::reais(100)).unwrap();
        os.enviar_para_aprovacao().unwrap();
        os.reprovar().unwrap();
        assert!(!os.aceita_edicao_de_dados());
        assert_eq!(
            os.completar_equipamento("Notebook").unwrap_err(),
            ErroOs::OrdemFinalizada
        );
        assert_eq!(
            os.complementar_defeito_relatado("mais detalhe")
                .unwrap_err(),
            ErroOs::OrdemFinalizada
        );
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
    fn faturar_direto_da_abertura_funciona_sem_passar_pelo_tramite() {
        // Pedido explícito do usuário (2026-09-15): laudo/orçamento/aprovação/execução são
        // opcionais — o balcão pode faturar assim que a OS abre.
        let mut os = os_aberta();
        os.faturar().unwrap();
        assert_eq!(os.estado, EstadoOs::Faturada);
    }

    #[test]
    fn faturar_cancelada_ou_reprovada_e_recusado() {
        let mut cancelada = os_aberta();
        cancelada.cancelar().unwrap();
        assert_eq!(
            cancelada.faturar().unwrap_err(),
            ErroOs::OsCanceladaOuReprovada
        );

        let mut reprovada = os_aberta();
        reprovada
            .adicionar_ao_orcamento(Dinheiro::reais(100))
            .unwrap();
        reprovada.enviar_para_aprovacao().unwrap();
        reprovada.reprovar().unwrap();
        assert_eq!(
            reprovada.faturar().unwrap_err(),
            ErroOs::OsCanceladaOuReprovada
        );
    }

    #[test]
    fn faturar_duas_vezes_e_recusado() {
        let mut os = os_aberta();
        os.faturar().unwrap();
        assert_eq!(os.faturar().unwrap_err(), ErroOs::OsJaFaturada);
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
    fn cancelar_a_partir_de_aprovada_ou_em_execucao_funciona() {
        // O cliente aprovou, mas a peça não chegou — precisa poder cancelar antes de
        // iniciar a execução, não só antes de aprovar.
        let mut os = os_aberta();
        os.adicionar_ao_orcamento(Dinheiro::reais(100)).unwrap();
        os.enviar_para_aprovacao().unwrap();
        os.aprovar("João Silva - CPF 529.982.247-25").unwrap();
        assert_eq!(os.estado, EstadoOs::Aprovada);
        os.cancelar().unwrap();
        assert_eq!(os.estado, EstadoOs::Cancelada);

        // Já em execução (com peça possivelmente aplicada) — o comando
        // `CancelarOrdemServico` é quem estorna o consumo; o domínio só precisa aceitar a
        // transição.
        let mut os2 = os_aberta();
        os2.adicionar_ao_orcamento(Dinheiro::reais(100)).unwrap();
        os2.enviar_para_aprovacao().unwrap();
        os2.aprovar("João Silva - CPF 529.982.247-25").unwrap();
        os2.iniciar_execucao().unwrap();
        assert_eq!(os2.estado, EstadoOs::EmExecucao);
        os2.cancelar().unwrap();
        assert_eq!(os2.estado, EstadoOs::Cancelada);
    }

    #[test]
    fn desfaturar_volta_para_concluida_e_aceita_ajuste_de_itens() {
        let mut os = os_aberta();
        os.faturar().unwrap();
        assert_eq!(os.estado, EstadoOs::Faturada);

        os.desfaturar().unwrap();
        assert_eq!(os.estado, EstadoOs::Concluida);
        assert!(os.estado.aceita_ajuste_de_itens());

        // Corrige um valor sem precisar repetir laudo/aprovação/execução.
        os.adicionar_ao_orcamento(Dinheiro::reais(50)).unwrap();
        os.faturar().unwrap();
        assert_eq!(os.estado, EstadoOs::Faturada);
    }

    #[test]
    fn desfaturar_fora_de_faturada_e_recusado() {
        let mut os = os_aberta();
        assert_eq!(os.desfaturar().unwrap_err(), ErroOs::OsNaoFaturada);
    }

    #[test]
    fn reabrir_volta_cancelada_para_aberta() {
        let mut os = os_aberta();
        os.cancelar().unwrap();
        assert_eq!(os.estado, EstadoOs::Cancelada);

        os.reabrir().unwrap();
        assert_eq!(os.estado, EstadoOs::Aberta);
    }

    #[test]
    fn reabrir_fora_de_cancelada_e_recusado() {
        let mut os = os_aberta();
        assert_eq!(os.reabrir().unwrap_err(), ErroOs::OsNaoCancelada);
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
