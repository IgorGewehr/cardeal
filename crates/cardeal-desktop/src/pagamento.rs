//! O "como vai ser pago" compartilhado entre faturar OS e dar baixa no Financeiro: monta o
//! organismo [`SeletorPagamento`] com os meios de `mod-financeiro`, carrega as contas
//! bancárias candidatas e cria uma conta nova no meio do fluxo, sem sair do diálogo.

use cardeal_cliente::{MotorLocal, SessaoLocal};
use cardeal_kernel::{Data, Dinheiro, Id};
use cardeal_ui::atoms::Botao;
use cardeal_ui::molecules::Campo;
use cardeal_ui::organisms::{notificar, CondicaoPagamento, Notificacao, SeletorPagamento};
use cardeal_ui::tokens::Espaco;
use eframe::egui;
use mod_financeiro::{
    ContaBancariaCriada, ContasDisponiveis, CriarContaBancaria, ItemContaDisponivel, MeioPagamento,
};

/// A condição de pagamento com os tipos do financeiro.
pub type Condicao = CondicaoPagamento<MeioPagamento, Id>;

/// Estado completo do bloco de pagamento de um diálogo: a condição escolhida, as contas
/// bancárias (carregadas uma vez) e o miniformulário de conta nova.
#[derive(Debug, Clone)]
pub struct EstadoPagamento {
    /// O que foi escolhido.
    pub condicao: Condicao,
    contas: Vec<ItemContaDisponivel>,
    contas_carregadas: bool,
    criando_conta: bool,
    nova_conta_nome: String,
}

impl EstadoPagamento {
    /// À vista, começando pelo meio dado.
    pub fn novo(meio: MeioPagamento) -> Self {
        Self {
            condicao: Condicao::com_meio(meio),
            contas: Vec::new(),
            contas_carregadas: false,
            criando_conta: false,
            nova_conta_nome: String::new(),
        }
    }

    /// A conta de destino a mandar para o backend: só quando o meio cai no banco (Dinheiro vai
    /// sempre para o Caixa, pelo papel do meio).
    pub fn conta_destino(&self) -> Option<Id> {
        match self.condicao.meio {
            Some(MeioPagamento::Pix | MeioPagamento::Cartao) => self.condicao.conta,
            _ => None,
        }
    }

    /// O meio escolhido, ou um aviso para a tela mostrar.
    ///
    /// # Errors
    /// Mensagem pronta para o usuário quando falta o meio, ou a conta de um meio bancário
    /// com mais de uma conta cadastrada.
    pub fn meio_validado(&self) -> Result<MeioPagamento, &'static str> {
        let meio = self.condicao.meio.ok_or("Escolha o meio de pagamento.")?;
        if matches!(meio, MeioPagamento::Pix | MeioPagamento::Cartao)
            && self.contas.len() > 1
            && self.condicao.conta.is_none()
        {
            return Err("Escolha em qual conta bancária o valor caiu.");
        }
        Ok(meio)
    }

    /// Parcelas, 1º vencimento e intervalo de uma condição a prazo.
    ///
    /// # Errors
    /// Mensagem pronta para o usuário quando algum campo não é válido.
    pub fn prazo_validado(&self) -> Result<(u16, Data, i32), &'static str> {
        let c = &self.condicao;
        let parcelas = c
            .parcelas
            .trim()
            .parse::<u16>()
            .ok()
            .filter(|n| (1..=60).contains(n))
            .ok_or("Parcelas: um número de 1 a 60.")?;
        let vencimento = c
            .primeiro_vencimento
            .parse::<Data>()
            .map_err(|_| "Informe o 1º vencimento (dd/mm/aaaa).")?;
        let intervalo = if parcelas == 1 {
            0
        } else {
            c.intervalo_dias
                .trim()
                .parse::<i32>()
                .ok()
                .filter(|d| (1..=365).contains(d))
                .ok_or("Intervalo: de 1 a 365 dias.")?
        };
        Ok((parcelas, vencimento, intervalo))
    }

    /// Desenha o bloco. `com_prazo` liga a escolha "À vista / A prazo".
    pub fn mostrar(
        &mut self,
        ui: &mut egui::Ui,
        motor: &MotorLocal,
        sessao: &SessaoLocal,
        id: &str,
        com_prazo: bool,
    ) {
        if !self.contas_carregadas {
            self.contas_carregadas = true;
            match motor.consultar(
                sessao,
                "financeiro.contas_disponiveis.v1",
                &ContasDisponiveis,
            ) {
                Ok(todas) => {
                    let todas: Vec<ItemContaDisponivel> = todas;
                    self.contas = todas.into_iter().filter(|c| !c.e_caixa).collect();
                }
                Err(e) => notificar(
                    ui.ctx(),
                    Notificacao::erro("Não foi possível carregar as contas bancárias")
                        .detalhe(e.mensagem),
                ),
            }
        }
        if self.condicao.primeiro_vencimento.is_empty() {
            self.condicao.primeiro_vencimento = Data::hoje(cardeal_kernel::Fuso::BRASILIA)
                .mais_dias(30)
                .to_string();
        }

        let mut seletor = SeletorPagamento::novo(id, &mut self.condicao)
            .meio(MeioPagamento::Dinheiro, "Dinheiro", false)
            .meio(MeioPagamento::Pix, "Pix", true)
            .meio(MeioPagamento::Cartao, "Cartão", true)
            .contas(
                self.contas
                    .iter()
                    .map(|c| (c.conta, format!("{} ({})", c.nome, c.codigo))),
            );
        if com_prazo {
            seletor = seletor.com_prazo();
        }
        if seletor.mostrar(ui).pediu_nova_conta {
            self.criando_conta = true;
        }

        if self.criando_conta {
            ui.add_space(Espaco::E8);
            ui.horizontal(|ui| {
                ui.add(
                    Campo::novo("Nova conta bancária", &mut self.nova_conta_nome)
                        .marcador("ex.: Nubank, Banco do Brasil"),
                );
                if ui.add(Botao::primario("Criar")).clicked() {
                    self.criar_conta(ui.ctx(), motor, sessao);
                }
                if ui.add(Botao::fantasma("Cancelar")).clicked() {
                    self.criando_conta = false;
                }
            });
        }
    }

    fn criar_conta(&mut self, ctx: &egui::Context, motor: &MotorLocal, sessao: &SessaoLocal) {
        let nome = self.nova_conta_nome.trim().to_owned();
        if nome.is_empty() {
            notificar(ctx, Notificacao::aviso("Informe o nome da conta."));
            return;
        }
        match motor.executar(
            sessao,
            "financeiro.criar_conta_bancaria.v1",
            &CriarContaBancaria { nome: nome.clone() },
        ) {
            Ok(c) => {
                let c: ContaBancariaCriada = c;
                self.contas.push(ItemContaDisponivel {
                    conta: c.conta,
                    codigo: c.codigo,
                    nome,
                    saldo: Dinheiro::ZERO,
                    e_caixa: false,
                });
                self.condicao.conta = Some(c.conta);
                self.criando_conta = false;
                self.nova_conta_nome.clear();
                notificar(ctx, Notificacao::sucesso("Conta criada"));
            }
            Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
        }
    }
}
