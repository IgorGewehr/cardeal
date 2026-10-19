//! A aba "Inventário": contagem cega com o local travado (`docs/modulos/estoque.md` §4,
//! §10 "Inventário — contagem cega", §11.4–5).
//!
//! À esquerda, os inventários e o formulário de um novo (local, grupo opcional, descrição). À
//! direita, o selecionado: planejado → "Iniciar contagem" (tira a foto e trava o local); em
//! contagem → bipe ou busca, quantidade e "Registrar", com a lista do que já foi contado — a
//! quantidade do sistema **não aparece**; encerrado → as diferenças e o valor dos ajustes.

use super::*;
use cardeal_kernel::{Dinheiro, Fuso, Quantidade};
use cardeal_ui::molecules::{OpcaoBusca, SeletorBusca};
use cardeal_ui::organisms::{dialogo_confirmacao, Painel};
use mod_estoque::{
    CancelarInventario, ContagemIniciada, ContagemRegistrada, CriarInventario, EncerrarInventario,
    EstadoInventario, IniciarContagem, InventarioCriado, InventarioEncerradoResumo, Inventarios,
    PosicaoDoInventario, PosicaoInventario, RegistrarContagem, ResumoInventario,
};

/// Uma ação que pede confirmação antes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Confirmar {
    Iniciar,
    Encerrar,
    Cancelar,
}

/// O estado da aba.
#[derive(Default)]
pub(super) struct EstadoInventarioUi {
    pub(super) lista: Vec<ResumoInventario>,
    pub(super) selecionado: Option<Id>,
    pub(super) posicao: Option<PosicaoInventario>,
    // Novo inventário.
    pub(super) novo_local: Option<Id>,
    pub(super) novo_grupo: Option<Id>,
    pub(super) nova_descricao: String,
    // Contagem.
    pub(super) busca: String,
    pub(super) produto: Option<Id>,
    pub(super) quantidade: String,
    pub(super) confirmar: Option<Confirmar>,
    sel_grupo: cardeal_ui::organisms::EstadoSeletorArvore,
}

impl EstadoInventarioUi {
    pub(super) fn carregar(&mut self, motor: &Motor, sessao: &Sessao) {
        if let Ok(l) = motor.consultar(sessao, "estoque.inventarios.v1", &Inventarios) {
            self.lista = l;
        }
        if self.selecionado.is_none() {
            // Abre no que está em andamento, se houver.
            self.selecionado = self.lista.iter().find(|i| i.estado.aberto()).map(|i| i.id);
        }
        self.carregar_posicao(motor, sessao);
    }

    fn carregar_posicao(&mut self, motor: &Motor, sessao: &Sessao) {
        self.posicao = self.selecionado.and_then(|inventario| {
            motor
                .consultar(
                    sessao,
                    "estoque.posicao_do_inventario.v1",
                    &PosicaoDoInventario { inventario },
                )
                .ok()
        });
    }

    fn selecionar(&mut self, motor: &Motor, sessao: &Sessao, id: Id) {
        self.selecionado = Some(id);
        self.limpar_contagem();
        self.carregar_posicao(motor, sessao);
    }

    fn limpar_contagem(&mut self) {
        self.busca.clear();
        self.produto = None;
        self.quantidade.clear();
    }
}

fn rotulo_estado(e: EstadoInventario) -> Etiqueta {
    match e {
        EstadoInventario::Planejado => Etiqueta::neutra("planejado"),
        EstadoInventario::EmContagem | EstadoInventario::Congelado => {
            Etiqueta::atencao("em contagem")
        }
        EstadoInventario::Conferido => Etiqueta::info("conferido"),
        EstadoInventario::Encerrado => Etiqueta::positiva("encerrado"),
        EstadoInventario::Cancelado => Etiqueta::neutra("cancelado"),
    }
}

/// Executa um comando e, dando certo, recarrega a aba e o estoque e avisa.
fn rodar<C>(
    ctx: &egui::Context,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaEstoque,
    nome: &str,
    comando: &C,
    sucesso: impl FnOnce(&C::Saida) -> String,
) -> bool
where
    C: cardeal_modkit::Comando + serde::Serialize,
    C::Saida: serde::de::DeserializeOwned,
{
    match motor.executar(sessao, nome, comando) {
        Ok(r) => {
            notificar(ctx, Notificacao::sucesso(sucesso(&r)));
            estado.carregar(motor, sessao);
            estado.inv.carregar(motor, sessao);
            true
        }
        Err(e) => {
            notificar(ctx, Notificacao::erro(e.mensagem));
            false
        }
    }
}

pub(super) fn aba(
    ui: &mut egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaEstoque,
) {
    ui.add(Rotulo::apoio(
        "Contagem cega: ao iniciar, o sistema tira a foto do saldo e trava o local — entradas, \
         saídas e ajustes ali ficam bloqueados até encerrar ou cancelar. A quantidade do sistema \
         só aparece depois de encerrado, para não induzir a contagem.",
    ));
    ui.add_space(Espaco::E12);
    ui.columns(2, |c| {
        lista(&mut c[0], motor, sessao, estado);
        detalhe(&mut c[1], motor, sessao, estado);
    });
    confirmacoes(ui.ctx(), motor, sessao, estado);
}

fn lista(ui: &mut egui::Ui, motor: &Motor, sessao: &Sessao, estado: &mut EstadoTelaEstoque) {
    Painel::novo().plano().mostrar(ui, |ui| {
        ui.add(Rotulo::titulo_secao("Novo inventário"));
        ui.add_space(Espaco::E8);
        SeletorOpcao::novo("Local", &mut estado.inv.novo_local)
            .opcoes(estado.locais.iter().map(|l| (l.id, l.nome.clone())))
            .placeholder("Escolha o local a contar")
            .mostrar(ui);
        ui.add_space(Espaco::E8);
        let nos = estado.est.nos_arvore();
        SeletorEmArvore::novo(
            "Só este grupo (opcional)",
            &mut estado.inv.novo_grupo,
            &nos,
            &mut estado.inv.sel_grupo,
        )
        .marcador("O local inteiro")
        .opcao_vazia("O local inteiro")
        .mostrar(ui);
        ui.add_space(Espaco::E8);
        ui.add(
            Campo::novo("Descrição", &mut estado.inv.nova_descricao)
                .marcador("ex.: Balanço de outubro"),
        );
        ui.add_space(Espaco::E8);
        let pode = estado.inv.novo_local.is_some();
        if ui
            .add_enabled(pode, Botao::primario("Criar inventário"))
            .clicked()
        {
            if let Some(local) = estado.inv.novo_local {
                let comando = CriarInventario {
                    local,
                    grupo: estado.inv.novo_grupo,
                    descricao: estado.inv.nova_descricao.clone(),
                };
                let mut criado = None;
                if rodar(
                    ui.ctx(),
                    motor,
                    sessao,
                    estado,
                    "estoque.criar_inventario.v1",
                    &comando,
                    |r: &InventarioCriado| {
                        criado = Some(r.inventario);
                        "Inventário criado: inicie a contagem quando estiver pronto".to_owned()
                    },
                ) {
                    estado.inv.nova_descricao.clear();
                    if let Some(id) = criado {
                        estado.inv.selecionar(motor, sessao, id);
                    }
                }
            }
        }
    });
    ui.add_space(Espaco::E12);

    let selecionado = estado
        .inv
        .selecionado
        .and_then(|s| estado.inv.lista.iter().position(|i| i.id == s));
    let fuso = Fuso::BRASILIA;
    let resposta = Grade::nova(vec![
        ColunaGrade::nova("Inventário"),
        ColunaGrade::nova("Local").largura(110.0),
        ColunaGrade::nova("Situação").largura(110.0),
        ColunaGrade::nova("Contados").largura(90.0).numero(),
    ])
    .id_salt("inventarios")
    .selecionavel(selecionado)
    .vazio("Nenhum inventário ainda.")
    .mostrar(ui, estado.inv.lista.len(), |i, row| {
        let l = &estado.inv.lista[i];
        row.col(|ui| {
            ui.vertical(|ui| {
                ui.add(Rotulo::interface(l.descricao.clone()));
                ui.add(Rotulo::apoio(l.criado_em.formatar(fuso)));
            });
        });
        row.col(|ui| {
            ui.add(Rotulo::interface(l.nome_local.clone()));
        });
        row.col(|ui| {
            ui.add(rotulo_estado(l.estado));
        });
        row.col(|ui| {
            ui.add(Rotulo::interface(format!("{}/{}", l.contados, l.itens)));
        });
    });
    if let Some(i) = resposta.linha_clicada {
        if let Some(id) = estado.inv.lista.get(i).map(|l| l.id) {
            estado.inv.selecionar(motor, sessao, id);
        }
    }
}

fn detalhe(ui: &mut egui::Ui, motor: &Motor, sessao: &Sessao, estado: &mut EstadoTelaEstoque) {
    let Some(pos) = estado.inv.posicao.clone() else {
        EstadoVazio::novo(
            Icone::Estoque,
            "Escolha um inventário na lista, ou crie um novo ao lado.",
        )
        .mostrar(ui);
        return;
    };
    let r = &pos.resumo;
    Painel::novo().plano().mostrar(ui, |ui| {
        ui.horizontal(|ui| {
            ui.add(Rotulo::titulo_secao(r.descricao.clone()));
            ui.add(rotulo_estado(r.estado));
        });
        ui.add(Rotulo::apoio(format!(
            "{} · {} produto(s) na lista",
            r.nome_local, r.itens
        )));
        ui.add_space(Espaco::E12);
        match r.estado {
            EstadoInventario::Planejado => planejado(ui, estado),
            EstadoInventario::EmContagem | EstadoInventario::Congelado => {
                contagem(ui, motor, sessao, estado, &pos);
            }
            EstadoInventario::Conferido | EstadoInventario::Encerrado => resultado(ui, &pos),
            EstadoInventario::Cancelado => {
                ui.add(Rotulo::apoio(
                    "Cancelado: o local foi liberado e nenhum saldo foi ajustado.",
                ));
            }
        }
    });
}

fn planejado(ui: &mut egui::Ui, estado: &mut EstadoTelaEstoque) {
    ui.add(Rotulo::interface(
        "Quando a equipe estiver pronta para contar, inicie: a lista sai do saldo de agora e o \
         local fica travado até o fim.",
    ));
    ui.add_space(Espaco::E8);
    ui.horizontal(|ui| {
        if ui.add(Botao::primario("Iniciar contagem")).clicked() {
            estado.inv.confirmar = Some(Confirmar::Iniciar);
        }
        if ui.add(Botao::fantasma("Cancelar inventário")).clicked() {
            estado.inv.confirmar = Some(Confirmar::Cancelar);
        }
    });
}

fn contagem(
    ui: &mut egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaEstoque,
    pos: &PosicaoInventario,
) {
    let r = &pos.resumo;
    FaixaKpi::nova(vec![
        CartaoKpi::contagem("Contados", r.contados).tom(Tom::Positivo),
        CartaoKpi::contagem("Pendentes", r.itens.saturating_sub(r.contados)).tom(
            if r.contados >= r.itens {
                Tom::Neutro
            } else {
                Tom::Atencao
            },
        ),
    ])
    .mostrar(ui);
    ui.add_space(Espaco::E8);

    // O bipe: código de barras exato seleciona direto.
    if estado.inv.produto.is_none() {
        let codigo = estado.inv.busca.trim();
        if !codigo.is_empty() {
            if let Some(p) = estado
                .produtos
                .iter()
                .find(|p| p.codigo_barras.as_deref() == Some(codigo))
            {
                estado.inv.produto = Some(p.produto);
            }
        }
    }
    let opcoes: Vec<OpcaoBusca<Id>> = estado
        .produtos
        .iter()
        .map(|p| {
            let o = OpcaoBusca::nova(p.produto, p.nome.clone());
            match &p.codigo_barras {
                Some(c) => o.subtitulo(c.clone()),
                None => o,
            }
        })
        .collect();
    SeletorBusca::novo(
        "Produto (bipe ou digite)",
        &mut estado.inv.busca,
        &mut estado.inv.produto,
    )
    .opcoes(opcoes)
    .marcador("Código de barras ou nome")
    .mostrar(ui);
    ui.add_space(Espaco::E8);
    ui.horizontal(|ui| {
        ui.add(Campo::novo("Quantidade contada", &mut estado.inv.quantidade).marcador("0"));
    });
    ui.add_space(Espaco::E8);
    ui.horizontal(|ui| {
        let pode = estado.inv.produto.is_some() && !estado.inv.quantidade.trim().is_empty();
        if ui
            .add_enabled(pode, Botao::primario("Registrar contagem"))
            .clicked()
        {
            registrar(ui.ctx(), motor, sessao, estado, r.id);
        }
        if ui.add(Botao::secundario("Encerrar e ajustar")).clicked() {
            estado.inv.confirmar = Some(Confirmar::Encerrar);
        }
        if ui.add(Botao::fantasma("Cancelar inventário")).clicked() {
            estado.inv.confirmar = Some(Confirmar::Cancelar);
        }
    });
    ui.add_space(Espaco::E12);

    Grade::nova(vec![
        ColunaGrade::nova("Produto"),
        ColunaGrade::nova("Código").largura(130.0),
        ColunaGrade::nova("Contado").largura(110.0).numero(),
        ColunaGrade::nova("Situação").largura(110.0),
    ])
    .id_salt("inventario-contagem")
    .vazio("Nada na lista.")
    .mostrar(ui, pos.itens.len(), |i, row| {
        let it = &pos.itens[i];
        row.col(|ui| {
            ui.add(Rotulo::interface(it.nome.clone()));
        });
        row.col(|ui| {
            ui.add(Rotulo::campo(it.codigo_barras.clone().unwrap_or_default()));
        });
        row.col(|ui| {
            ui.add(Rotulo::interface(it.contada.map_or_else(
                || "—".to_owned(),
                |q| format!("{q} {}", it.unidade),
            )));
        });
        row.col(|ui| {
            if it.contada.is_some() {
                ui.add(Etiqueta::positiva("registrado"));
            } else {
                ui.add(Etiqueta::neutra("pendente"));
            }
        });
    });
}

pub(super) fn registrar(
    ctx: &egui::Context,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaEstoque,
    inventario: Id,
) {
    let Some(produto) = estado.inv.produto else {
        return;
    };
    let quantidade = match Quantidade::de_str(estado.inv.quantidade.trim()) {
        Ok(q) => q,
        Err(_) => {
            notificar(
                ctx,
                Notificacao::erro("Digite a quantidade como número, por exemplo 12 ou 2,5"),
            );
            return;
        }
    };
    let comando = RegistrarContagem {
        inventario,
        produto,
        quantidade,
    };
    if rodar(
        ctx,
        motor,
        sessao,
        estado,
        "estoque.registrar_contagem.v1",
        &comando,
        |r: &ContagemRegistrada| {
            let extra = if r.incluido {
                " (o produto não estava na lista e entrou agora)"
            } else {
                ""
            };
            format!("Contagem registrada{extra}: {}/{}", r.contados, r.itens)
        },
    ) {
        estado.inv.limpar_contagem();
    }
}

fn resultado(ui: &mut egui::Ui, pos: &PosicaoInventario) {
    let com_diferenca: Vec<_> = pos
        .itens
        .iter()
        .filter(|i| i.diferenca.is_some_and(|d| !d.e_zero()))
        .collect();
    let (mut sobras, mut faltas) = (Dinheiro::ZERO, Dinheiro::ZERO);
    for i in &com_diferenca {
        let valor = i.valor_ajuste.unwrap_or(Dinheiro::ZERO);
        if i.diferenca.is_some_and(Quantidade::e_positiva) {
            sobras += valor;
        } else {
            faltas += valor;
        }
    }
    FaixaKpi::nova(vec![
        CartaoKpi::contagem("Com diferença", com_diferenca.len()).tom(Tom::Neutro),
        CartaoKpi::novo("Sobras", sobras).tom(Tom::Positivo),
        CartaoKpi::novo("Faltas", faltas).tom(Tom::Negativo),
    ])
    .mostrar(ui);
    ui.add_space(Espaco::E8);
    ui.add(Rotulo::apoio(
        "Os ajustes já estão no saldo e no razão, ao custo médio: sobra em outras receitas, falta \
         em perdas de estoque.",
    ));
    ui.add_space(Espaco::E8);
    Grade::nova(vec![
        ColunaGrade::nova("Produto"),
        ColunaGrade::nova("Sistema").largura(100.0).numero(),
        ColunaGrade::nova("Contado").largura(100.0).numero(),
        ColunaGrade::nova("Diferença").largura(100.0).numero(),
        ColunaGrade::nova("Valor").largura(110.0).numero(),
    ])
    .id_salt("inventario-resultado")
    .vazio("Nada na lista.")
    .mostrar(ui, pos.itens.len(), |i, row| {
        let it = &pos.itens[i];
        let q = |v: Option<Quantidade>| v.map_or_else(|| "—".to_owned(), |q| q.to_string());
        row.col(|ui| {
            ui.add(Rotulo::interface(it.nome.clone()));
        });
        row.col(|ui| {
            ui.add(Rotulo::interface(q(it.sistema)));
        });
        row.col(|ui| {
            ui.add(Rotulo::interface(q(it.contada)));
        });
        row.col(|ui| match it.diferenca {
            Some(d) if d.e_positiva() => {
                ui.add(Etiqueta::positiva(format!("+{d}")));
            }
            Some(d) if d.e_negativa() => {
                ui.add(Etiqueta::negativa(d.to_string()));
            }
            _ => {
                ui.add(Rotulo::interface("0"));
            }
        });
        row.col(|ui| match it.valor_ajuste {
            Some(v) => {
                ui.add(cardeal_ui::atoms::ValorDinheiro::novo(v).neutro());
            }
            None => {
                ui.add(Rotulo::interface("—"));
            }
        });
    });
}

fn confirmacoes(
    ctx: &egui::Context,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaEstoque,
) {
    let Some(acao) = estado.inv.confirmar else {
        return;
    };
    let Some(inventario) = estado.inv.selecionado else {
        estado.inv.confirmar = None;
        return;
    };
    let pendentes = estado
        .inv
        .posicao
        .as_ref()
        .map_or(0, |p| p.resumo.itens.saturating_sub(p.resumo.contados));
    let (titulo, mensagem, botao) = match acao {
        Confirmar::Iniciar => (
            "Iniciar contagem",
            "A lista sai do saldo de agora e o local fica travado: nenhuma entrada, saída ou \
             ajuste ali até encerrar ou cancelar o inventário."
                .to_owned(),
            "Iniciar e travar o local",
        ),
        Confirmar::Encerrar => (
            "Encerrar inventário",
            if pendentes > 0 {
                format!(
                    "Ainda há {pendentes} produto(s) sem contagem: conte todos antes de \
                     encerrar (o que não está na prateleira se conta como 0)."
                )
            } else {
                "As diferenças viram ajustes no saldo e lançamentos no razão, ao custo médio, \
                 e o local é liberado. Não dá para desfazer."
                    .to_owned()
            },
            "Encerrar e ajustar",
        ),
        Confirmar::Cancelar => (
            "Cancelar inventário",
            "O local é liberado e nenhum saldo é ajustado; as contagens feitas ficam só no \
             histórico."
                .to_owned(),
            "Cancelar inventário",
        ),
    };
    let resposta = dialogo_confirmacao(ctx, titulo, &mensagem, botao);
    if resposta.confirmado {
        match acao {
            Confirmar::Iniciar => {
                rodar(
                    ctx,
                    motor,
                    sessao,
                    estado,
                    "estoque.iniciar_contagem.v1",
                    &IniciarContagem { inventario },
                    |r: &ContagemIniciada| {
                        format!("Contagem iniciada: {} produto(s) na lista", r.itens)
                    },
                );
            }
            Confirmar::Encerrar => {
                rodar(
                    ctx,
                    motor,
                    sessao,
                    estado,
                    "estoque.encerrar_inventario.v1",
                    &EncerrarInventario { inventario },
                    |r: &InventarioEncerradoResumo| {
                        format!(
                            "Inventário encerrado: {} ajuste(s) — sobras {}, faltas {}",
                            r.ajustes, r.sobras, r.faltas
                        )
                    },
                );
            }
            Confirmar::Cancelar => {
                rodar(
                    ctx,
                    motor,
                    sessao,
                    estado,
                    "estoque.cancelar_inventario.v1",
                    &CancelarInventario { inventario },
                    |_: &()| "Inventário cancelado: o local foi liberado".to_owned(),
                );
            }
        }
    }
    if resposta.fechar || resposta.confirmado {
        estado.inv.confirmar = None;
    }
}
