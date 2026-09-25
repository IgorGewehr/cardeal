//! Camada 3 (organisms) — `SeletorPagamento`: o bloco "como vai ser pago" que faturar uma
//! OS e dar baixa numa parcela repetiam cada um do seu jeito (botões soltos num, dropdown no
//! outro, conta bancária só num deles).
//!
//! Três perguntas, na ordem em que o balcão pensa:
//! 1. **À vista ou a prazo?** — só quando a tela liga [`SeletorPagamento::com_prazo`]. A prazo
//!    pede parcelas, 1º vencimento e intervalo, e não pergunta meio (ninguém pagou ainda).
//! 2. **Meio** — pílulas (Dinheiro / Pix / Cartão), genéricas sobre o tipo do meio.
//! 3. **Conta** — só para meios que caem numa conta bancária, e só quando há mais de uma
//!    candidata (com uma só, ela é escolhida sozinha). Sem nenhuma, o seletor devolve
//!    [`RespostaPagamento::pediu_nova_conta`] para a tela abrir o cadastro rápido.
//!
//! O componente não conhece `mod-financeiro`: a tela diz quais meios existem e quais pedem
//! conta. As strings de parcelas/data ficam no estado para a tela converter (e validar) na hora
//! de confirmar, como qualquer `Campo`.

use egui::Ui;

use crate::atoms::{Botao, Rotulo};
use crate::molecules::{Abas, Campo, Mascara, SeletorOpcao};
use crate::tokens::Espaco;

/// O que o usuário escolheu — vive no estado da tela entre quadros.
#[derive(Debug, Clone)]
pub struct CondicaoPagamento<M, C> {
    /// `true` = a prazo (gera parcelas em aberto); `false` = à vista, pago agora.
    pub a_prazo: bool,
    /// O meio escolhido (só relevante à vista).
    pub meio: Option<M>,
    /// A conta de destino, para meios que pedem conta.
    pub conta: Option<C>,
    /// Quantidade de parcelas (texto do campo).
    pub parcelas: String,
    /// Vencimento da 1ª parcela, `dd/mm/aaaa`.
    pub primeiro_vencimento: String,
    /// Dias entre parcelas.
    pub intervalo_dias: String,
}

impl<M, C> Default for CondicaoPagamento<M, C> {
    fn default() -> Self {
        Self {
            a_prazo: false,
            meio: None,
            conta: None,
            parcelas: "1".to_owned(),
            primeiro_vencimento: String::new(),
            intervalo_dias: "30".to_owned(),
        }
    }
}

impl<M, C> CondicaoPagamento<M, C> {
    /// Começa à vista, já com um meio escolhido (o mais comum da tela).
    #[must_use]
    pub fn com_meio(meio: M) -> Self {
        Self {
            meio: Some(meio),
            ..Self::default()
        }
    }
}

/// O que aconteceu neste quadro.
#[derive(Debug, Clone, Copy, Default)]
pub struct RespostaPagamento {
    /// O usuário pediu para cadastrar uma conta bancária (não havia nenhuma, ou clicou em
    /// "+ Nova conta").
    pub pediu_nova_conta: bool,
}

/// Quando a condição é "à vista"/"a prazo".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Modo {
    AVista,
    APrazo,
}

/// O seletor de condição, meio e conta de pagamento.
#[must_use]
pub struct SeletorPagamento<'a, M, C> {
    id: &'a str,
    condicao: &'a mut CondicaoPagamento<M, C>,
    meios: Vec<(M, String, bool)>,
    contas: Vec<(C, String)>,
    com_prazo: bool,
}

impl<'a, M: PartialEq + Copy, C: PartialEq + Copy> SeletorPagamento<'a, M, C> {
    /// Um seletor ligado a `condicao`. `id` precisa ser único na tela (é o sal das pílulas).
    pub fn novo(id: &'a str, condicao: &'a mut CondicaoPagamento<M, C>) -> Self {
        Self {
            id,
            condicao,
            meios: Vec::new(),
            contas: Vec::new(),
            com_prazo: false,
        }
    }

    /// Acrescenta um meio; `pede_conta` = cai numa conta bancária (Pix, cartão).
    pub fn meio(mut self, valor: M, rotulo: impl Into<String>, pede_conta: bool) -> Self {
        self.meios.push((valor, rotulo.into(), pede_conta));
        self
    }

    /// As contas bancárias candidatas a receber um meio que pede conta.
    pub fn contas<I, S>(mut self, it: I) -> Self
    where
        I: IntoIterator<Item = (C, S)>,
        S: Into<String>,
    {
        self.contas
            .extend(it.into_iter().map(|(c, s)| (c, s.into())));
        self
    }

    /// Mostra também a escolha "À vista / A prazo" com parcelas e vencimento.
    pub const fn com_prazo(mut self) -> Self {
        self.com_prazo = true;
        self
    }

    /// Desenha o bloco.
    pub fn mostrar(self, ui: &mut Ui) -> RespostaPagamento {
        let Self {
            id,
            condicao,
            meios,
            contas,
            com_prazo,
        } = self;
        let mut resposta = RespostaPagamento::default();

        if com_prazo {
            let atual = if condicao.a_prazo {
                Modo::APrazo
            } else {
                Modo::AVista
            };
            if let Some(m) = Abas::nova(&[(Modo::AVista, "À vista"), (Modo::APrazo, "A prazo")])
                .selecionada(atual)
                .id_salt(id)
                .mostrar(ui)
            {
                condicao.a_prazo = m == Modo::APrazo;
            }
            ui.add_space(Espaco::E12);
            if condicao.a_prazo {
                ui.columns(3, |c| {
                    c[0].add(Campo::novo("Parcelas", &mut condicao.parcelas).marcador("1"));
                    c[1].add(
                        Campo::novo("1º vencimento", &mut condicao.primeiro_vencimento)
                            .mascara(Mascara::Data),
                    );
                    c[2].add(
                        Campo::novo("Intervalo (dias)", &mut condicao.intervalo_dias)
                            .marcador("30"),
                    );
                });
                return resposta;
            }
        }

        ui.add(Rotulo::campo("Meio de pagamento"));
        ui.add_space(Espaco::E4);
        // A pílula sempre aponta para algum meio; sem escolha prévia, é o primeiro — melhor
        // que desenhar a pílula num meio que o estado não registrou.
        if condicao.meio.is_none() {
            condicao.meio = meios.first().map(|(m, _, _)| *m);
        }
        let itens: Vec<(M, &str)> = meios.iter().map(|(m, r, _)| (*m, r.as_str())).collect();
        let sal_meio = format!("{id}-meio");
        let mut abas = Abas::nova(&itens).id_salt(&sal_meio);
        if let Some(m) = condicao.meio {
            abas = abas.selecionada(m);
        }
        if let Some(m) = abas.mostrar(ui) {
            condicao.meio = Some(m);
        }

        let pede_conta = condicao
            .meio
            .and_then(|m| meios.iter().find(|(v, _, _)| *v == m))
            .is_some_and(|(_, _, p)| *p);
        if !pede_conta {
            return resposta;
        }

        ui.add_space(Espaco::E12);
        // A escolha sai sozinha quando só há uma conta — e se a escolhida sumiu da lista.
        if !condicao
            .conta
            .is_some_and(|c| contas.iter().any(|(v, _)| *v == c))
        {
            condicao.conta = (contas.len() == 1).then(|| contas[0].0);
        }
        if contas.is_empty() {
            ui.add(Rotulo::campo("Nenhuma conta bancária cadastrada"));
            ui.add_space(Espaco::E4);
            if ui.add(Botao::secundario("+ Cadastrar conta")).clicked() {
                resposta.pediu_nova_conta = true;
            }
        } else {
            SeletorOpcao::novo("Conta bancária", &mut condicao.conta)
                .opcoes(contas.iter().map(|(c, s)| (*c, s.clone())))
                .mostrar(ui);
            ui.add_space(Espaco::E4);
            if ui.add(Botao::fantasma("+ Nova conta").pequeno()).clicked() {
                resposta.pediu_nova_conta = true;
            }
        }
        resposta
    }
}
