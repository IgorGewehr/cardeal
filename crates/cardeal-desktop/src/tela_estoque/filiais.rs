//! A aba "Entre filiais" (só com mais de um CNPJ no grupo): enviar mercadoria a outra filial da
//! mesma raiz com a NF-e de transferência, e receber o que outra mandou
//! (`mod_compras::transferencia`).
//!
//! À esquerda, o envio: para qual filial, de qual local, os produtos e a observação; "Enviar com
//! NF-e" tira do estoque ao custo médio e põe a nota na fila. À direita, o que está a caminho (e
//! o que já chegou): a filial que recebe escolhe o local e recebe quando a NF-e está autorizada;
//! a que enviou reemite a rejeitada ou cancela o envio sem nota autorizada.

use super::*;
use cardeal_cliente::EmpresaDoGrupo;
use cardeal_ui::molecules::{OpcaoBusca, SeletorBusca};
use cardeal_ui::organisms::Painel;
use mod_compras::{
    CancelarTransferencia, EmitirNfeDaTransferencia, EnviarTransferencia, EstadoTransferencia,
    ItemATransferir, ItemDaTransferencia, ItensDaTransferencia, ReceberTransferencia,
    SituacaoDaNota, TransferenciaEntreFiliais, TransferenciaEnviada, TransferenciasEntreFiliais,
};

use super::transferencia::ItemNaLista;

/// O estado da aba.
#[derive(Default)]
pub(super) struct EstadoFiliais {
    /// As outras filiais do grupo (vazio: sem grupo, a aba some).
    outras: Vec<EmpresaDoGrupo>,
    pub(super) destino: Option<Id>,
    pub(super) local: Option<Id>,
    busca: String,
    produto: Option<Id>,
    quantidade: String,
    pub(super) itens: Vec<ItemNaLista>,
    observacao: String,
    lista: Vec<TransferenciaEntreFiliais>,
    todas: bool,
    selecionada: Option<Id>,
    itens_da_selecionada: Vec<ItemDaTransferencia>,
    local_receber: Option<Id>,
    motivo: String,
}

impl EstadoFiliais {
    /// Só o grupo: a aba aparece com outra filial.
    pub(super) fn conhecer(&mut self, motor: &Motor, sessao: &Sessao) {
        let ativa = motor.empresa_da_sessao(sessao);
        self.outras = motor
            .empresas_do_grupo(sessao)
            .map(|v| v.into_iter().filter(|e| Some(e.id) != ativa).collect())
            .unwrap_or_default();
    }

    /// Há outra filial para onde mandar (ou de onde receber)?
    pub(super) fn ha_grupo(&self) -> bool {
        !self.outras.is_empty()
    }

    pub(super) fn carregar(&mut self, motor: &Motor, sessao: &Sessao) {
        self.conhecer(motor, sessao);
        self.lista = motor
            .consultar(
                sessao,
                "compras.transferencias.v1",
                &TransferenciasEntreFiliais {
                    so_a_caminho: !self.todas,
                },
            )
            .unwrap_or_default();
        if self
            .selecionada
            .is_some_and(|s| !self.lista.iter().any(|t| t.id == s))
        {
            self.selecionada = None;
        }
        self.carregar_itens(motor, sessao);
    }

    fn carregar_itens(&mut self, motor: &Motor, sessao: &Sessao) {
        self.itens_da_selecionada = match self.selecionada {
            Some(t) => motor
                .consultar(
                    sessao,
                    "compras.itens_da_transferencia.v1",
                    &ItensDaTransferencia { transferencia: t },
                )
                .unwrap_or_default(),
            None => Vec::new(),
        };
    }

    /// Para os testes: as transferências carregadas.
    #[cfg(test)]
    pub(super) fn lista(&self) -> &[TransferenciaEntreFiliais] {
        &self.lista
    }

    /// Para os testes: escolhe uma transferência.
    #[cfg(test)]
    pub(super) fn selecionar(&mut self, motor: &Motor, sessao: &Sessao, t: Id) {
        self.selecionada = Some(t);
        self.carregar_itens(motor, sessao);
    }

    /// Põe o produto escolhido na lista (somando se já estiver).
    fn adicionar(&mut self, nome: &str) -> Result<(), String> {
        let Some(produto) = self.produto else {
            return Err("Escolha o produto".to_owned());
        };
        let quantidade = Quantidade::de_str(self.quantidade.trim())
            .ok()
            .filter(|q| q.e_positiva())
            .ok_or_else(|| "Digite a quantidade, maior que zero".to_owned())?;
        match self.itens.iter_mut().find(|i| i.produto == produto) {
            Some(i) => i.quantidade += quantidade,
            None => self.itens.push(ItemNaLista {
                produto,
                nome: nome.to_owned(),
                quantidade,
            }),
        }
        self.produto = None;
        self.busca.clear();
        self.quantidade.clear();
        Ok(())
    }
}

/// Envia o que está na lista; devolve o que o envio fez.
pub(super) fn enviar(
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaEstoque,
) -> Result<TransferenciaEnviada, String> {
    let f = &estado.filiais;
    let (Some(destino), Some(local)) = (f.destino, f.local) else {
        return Err("Escolha a filial e o local de onde sai".to_owned());
    };
    let comando = EnviarTransferencia {
        destino,
        local,
        itens: f
            .itens
            .iter()
            .map(|i| ItemATransferir {
                produto: i.produto,
                quantidade: i.quantidade,
            })
            .collect(),
        observacao: f.observacao.clone(),
    };
    let r: TransferenciaEnviada = motor
        .executar(sessao, "compras.enviar_transferencia.v1", &comando)
        .map_err(|e| e.mensagem)?;
    estado.filiais.itens.clear();
    estado.filiais.observacao.clear();
    estado.filiais.selecionada = Some(r.transferencia);
    estado.carregar(motor, sessao);
    estado.filiais.carregar(motor, sessao);
    Ok(r)
}

pub(super) fn aba(
    ui: &mut egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaEstoque,
) {
    ui.add(
        Rotulo::apoio(
            "Para outra filial da mesma raiz de CNPJ, com a NF-e de transferência (CFOP \
             5152/6152): a mercadoria sai ao custo médio e fica a caminho até a filial que \
             recebe dar entrada, com a nota autorizada. CNPJ de outra raiz é outra empresa: vai \
             por venda.",
        )
        .quebravel(),
    );
    ui.add_space(Espaco::E12);
    ui.columns(2, |c| {
        if sessao.concede("compras.transferencia.enviar") {
            formulario(&mut c[0], motor, sessao, estado);
        } else {
            c[0].add(Rotulo::apoio(
                "O seu usuário não envia transferências entre filiais.",
            ));
        }
        lista(&mut c[1], motor, sessao, estado);
    });
}

fn formulario(ui: &mut egui::Ui, motor: &Motor, sessao: &Sessao, estado: &mut EstadoTelaEstoque) {
    Painel::novo().plano().mostrar(ui, |ui| {
        ui.add(Rotulo::titulo_secao("Enviar para outra filial"));
        ui.add_space(Espaco::E8);
        let filiais: Vec<(Id, String)> = estado
            .filiais
            .outras
            .iter()
            .map(|e| {
                let cnpj = e.cnpj.clone().unwrap_or_default();
                (e.id, format!("{} {cnpj}", e.nome).trim().to_owned())
            })
            .collect();
        let locais: Vec<(Id, String)> = estado
            .locais
            .iter()
            .map(|l| (l.id, l.nome.clone()))
            .collect();
        ui.columns(2, |c| {
            SeletorOpcao::novo("Para", &mut estado.filiais.destino)
                .opcoes(filiais)
                .placeholder("Filial")
                .mostrar(&mut c[0]);
            SeletorOpcao::novo("De (local)", &mut estado.filiais.local)
                .opcoes(locais)
                .placeholder("Local")
                .mostrar(&mut c[1]);
        });
        ui.add_space(Espaco::E8);
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
            "Produto",
            &mut estado.filiais.busca,
            &mut estado.filiais.produto,
        )
        .opcoes(opcoes)
        .marcador("Código de barras ou nome")
        .mostrar(ui);
        ui.add_space(Espaco::E8);
        ui.horizontal(|ui| {
            ui.add(Campo::novo("Quantidade", &mut estado.filiais.quantidade).marcador("0"));
        });
        ui.add_space(Espaco::E4);
        if ui.add(Botao::secundario("Adicionar à lista")).clicked() {
            let nome = estado
                .filiais
                .produto
                .and_then(|p| estado.produtos.iter().find(|x| x.produto == p))
                .map(|p| p.nome.clone())
                .unwrap_or_default();
            if let Err(e) = estado.filiais.adicionar(&nome) {
                notificar(ui.ctx(), Notificacao::erro(e));
            }
        }
        ui.add_space(Espaco::E12);
        let mut remover = None;
        Grade::nova(vec![
            ColunaGrade::nova("Produto"),
            ColunaGrade::nova("Quantidade").largura(110.0).numero(),
            ColunaGrade::nova("").largura(90.0),
        ])
        .id_salt("filiais-itens")
        .vazio("Nenhum produto na lista ainda.")
        .mostrar(ui, estado.filiais.itens.len(), |i, row| {
            let it = &estado.filiais.itens[i];
            row.col(|ui| {
                ui.add(Rotulo::interface(it.nome.clone()));
            });
            row.col(|ui| {
                ui.add(Rotulo::interface(it.quantidade.to_string()));
            });
            row.col(|ui| {
                if ui.add(Botao::fantasma("Remover").pequeno()).clicked() {
                    remover = Some(i);
                }
            });
        });
        if let Some(i) = remover {
            estado.filiais.itens.remove(i);
        }
        ui.add_space(Espaco::E8);
        ui.add(
            Campo::novo("Observação (vai na NF-e)", &mut estado.filiais.observacao)
                .marcador("ex.: reposição da filial"),
        );
        ui.add_space(Espaco::E8);
        let f = &estado.filiais;
        let pode = f.destino.is_some() && f.local.is_some() && !f.itens.is_empty();
        if ui
            .add(Botao::primario("Enviar com NF-e").habilitado(pode))
            .clicked()
        {
            match enviar(motor, sessao, estado) {
                Ok(r) => {
                    let aviso = match &r.nota {
                        SituacaoDaNota::Rejeitada(p) => Notificacao::aviso(format!(
                            "Mercadoria a caminho ({}), mas a NF-e foi rejeitada: {p}",
                            r.valor.formatar_com_simbolo()
                        )),
                        _ => Notificacao::sucesso(format!(
                            "Mercadoria a caminho ({} ao custo); a NF-e vai para a SEFAZ",
                            r.valor.formatar_com_simbolo()
                        )),
                    };
                    notificar(ui.ctx(), aviso);
                    crate::fiscal_fundo::disparar(ui.ctx(), motor, sessao);
                }
                Err(e) => notificar(ui.ctx(), Notificacao::erro(e)),
            }
        }
    });
}

fn etiqueta(t: &TransferenciaEntreFiliais) -> Etiqueta {
    match (t.estado, &t.nota) {
        (EstadoTransferencia::Recebida, _) => Etiqueta::positiva("recebida"),
        (EstadoTransferencia::Cancelada, _) => Etiqueta::neutra("cancelada"),
        (EstadoTransferencia::Enviada, SituacaoDaNota::Autorizada) => {
            Etiqueta::info("a caminho · NF-e autorizada")
        }
        (EstadoTransferencia::Enviada, SituacaoDaNota::Rejeitada(_)) => {
            Etiqueta::negativa("a caminho · NF-e rejeitada")
        }
        (EstadoTransferencia::Enviada, n) => {
            Etiqueta::atencao(format!("a caminho · {}", n.rotulo()))
        }
    }
}

/// O que a pessoa pediu sobre a selecionada.
enum Pedido {
    Receber(Id, Id),
    EmitirDeNovo(Id),
    Cancelar(Id, String),
}

fn lista(ui: &mut egui::Ui, motor: &Motor, sessao: &Sessao, estado: &mut EstadoTelaEstoque) {
    ui.horizontal(|ui| {
        ui.add(Rotulo::titulo_secao(if estado.filiais.todas {
            "Transferências entre filiais"
        } else {
            "A caminho"
        }));
        let rot = if estado.filiais.todas {
            "Só as a caminho"
        } else {
            "Mostrar também as recebidas"
        };
        if ui.add(Botao::fantasma(rot).pequeno()).clicked() {
            estado.filiais.todas = !estado.filiais.todas;
            estado.filiais.carregar(motor, sessao);
        }
    });
    ui.add_space(Espaco::E8);
    let fuso = Fuso::BRASILIA;
    let f = &mut estado.filiais;
    let selecionada = f
        .selecionada
        .and_then(|s| f.lista.iter().position(|t| t.id == s));
    let resposta = Grade::nova(vec![
        ColunaGrade::nova("Quando").largura(130.0),
        ColunaGrade::nova("De → Para"),
        ColunaGrade::nova("Valor").largura(100.0).numero(),
        ColunaGrade::nova("Situação").largura(190.0),
    ])
    .id_salt("filiais-lista")
    .selecionavel(selecionada)
    .vazio("Nada a caminho.")
    .mostrar(ui, f.lista.len(), |i, row| {
        let t = &f.lista[i];
        row.col(|ui| {
            ui.add(Rotulo::interface(t.enviada_em.formatar(fuso)));
        });
        row.col(|ui| {
            ui.add(Rotulo::interface(format!(
                "{} → {}",
                t.origem_nome, t.destino_nome
            )));
        });
        row.col(|ui| {
            ui.add(cardeal_ui::atoms::ValorDinheiro::novo(t.valor).neutro());
        });
        row.col(|ui| {
            ui.add(etiqueta(t));
        });
    });
    if let Some(i) = resposta.linha_clicada {
        f.selecionada = f.lista.get(i).map(|t| t.id);
        f.carregar_itens(motor, sessao);
    }
    let Some(t) = f
        .selecionada
        .and_then(|s| f.lista.iter().find(|t| t.id == s))
        .cloned()
    else {
        return;
    };
    ui.add_space(Espaco::E12);
    if let Some(obs) = &t.observacao {
        ui.add(Rotulo::apoio(obs.clone()).quebravel());
    }
    if let Some(n) = t.numero_nota {
        ui.add(Rotulo::campo(format!("NF-e nº {n}")));
    }
    Grade::nova(vec![
        ColunaGrade::nova("Produto"),
        ColunaGrade::nova("Quantidade").largura(100.0).numero(),
        ColunaGrade::nova("Custo").largura(100.0).numero(),
        ColunaGrade::nova("Valor").largura(100.0).numero(),
    ])
    .id_salt("filiais-detalhe")
    .mostrar(ui, f.itens_da_selecionada.len(), |i, row| {
        let it = &f.itens_da_selecionada[i];
        row.col(|ui| {
            ui.add(Rotulo::interface(it.nome.clone()));
        });
        row.col(|ui| {
            ui.add(Rotulo::interface(it.quantidade.to_string()));
        });
        row.col(|ui| {
            ui.add(Rotulo::interface(it.custo_unitario.formatar_com_simbolo()));
        });
        row.col(|ui| {
            ui.add(cardeal_ui::atoms::ValorDinheiro::novo(it.valor).neutro());
        });
    });
    if t.estado != EstadoTransferencia::Enviada {
        return;
    }
    ui.add_space(Espaco::E12);
    let pedido = acoes(ui, sessao, &t, estado);
    if let Some(p) = pedido {
        let r = match p {
            Pedido::Receber(transferencia, local) => motor
                .executar(
                    sessao,
                    "compras.receber_transferencia.v1",
                    &ReceberTransferencia {
                        transferencia,
                        local,
                    },
                )
                .map(|v| format!("Recebida: {} entrou no estoque", v.formatar_com_simbolo())),
            Pedido::EmitirDeNovo(transferencia) => motor
                .executar(
                    sessao,
                    "compras.emitir_nfe_da_transferencia.v1",
                    &EmitirNfeDaTransferencia { transferencia },
                )
                .map(|s: SituacaoDaNota| match s {
                    SituacaoDaNota::Rejeitada(p) => format!("A NF-e continua rejeitada: {p}"),
                    s => format!("NF-e: {}", s.rotulo()),
                }),
            Pedido::Cancelar(transferencia, motivo) => motor
                .executar(
                    sessao,
                    "compras.cancelar_transferencia.v1",
                    &CancelarTransferencia {
                        transferencia,
                        motivo,
                    },
                )
                .map(|()| "Envio cancelado: a mercadoria voltou ao local".to_owned()),
        };
        match r {
            Ok(m) => {
                notificar(ui.ctx(), Notificacao::sucesso(m));
                crate::fiscal_fundo::disparar(ui.ctx(), motor, sessao);
                estado.filiais.motivo.clear();
                estado.carregar(motor, sessao);
                estado.filiais.carregar(motor, sessao);
            }
            Err(e) => notificar(ui.ctx(), Notificacao::erro(e.mensagem)),
        }
    }
}

/// As ações da selecionada, a caminho: receber (na que recebe) ou reemitir/cancelar (na que
/// enviou).
fn acoes(
    ui: &mut egui::Ui,
    sessao: &Sessao,
    t: &TransferenciaEntreFiliais,
    estado: &mut EstadoTelaEstoque,
) -> Option<Pedido> {
    let mut pedido = None;
    if !t.enviada_por_esta {
        if !sessao.concede("compras.transferencia.receber") {
            return None;
        }
        Painel::novo()
            .realce(Tom::Info)
            .compacto()
            .mostrar(ui, |ui| {
                if t.nota != SituacaoDaNota::Autorizada {
                    ui.add(
                        Rotulo::apoio(format!(
                        "A filial que enviou ainda não tem a NF-e autorizada ({}): a mercadoria \
                         só circula com ela.",
                        t.nota.rotulo()
                    ))
                        .quebravel(),
                    );
                    return;
                }
                let locais: Vec<(Id, String)> = estado
                    .locais
                    .iter()
                    .map(|l| (l.id, l.nome.clone()))
                    .collect();
                SeletorOpcao::novo("Receber no local", &mut estado.filiais.local_receber)
                    .opcoes(locais)
                    .placeholder("Local")
                    .mostrar(ui);
                ui.add_space(Espaco::E8);
                let local = estado.filiais.local_receber;
                if ui
                    .add(Botao::primario("Receber").habilitado(local.is_some()))
                    .clicked()
                {
                    pedido = local.map(|l| Pedido::Receber(t.id, l));
                }
            });
        return pedido;
    }
    if !sessao.concede("compras.transferencia.enviar") {
        return None;
    }
    Painel::novo().compacto().mostrar(ui, |ui| {
        match &t.nota {
            SituacaoDaNota::Autorizada => {
                ui.add(
                    Rotulo::apoio(
                        "NF-e autorizada: falta a filial receber. Para desfazer, cancele a NF-e \
                         em Notas fiscais (no prazo da SEFAZ).",
                    )
                    .quebravel(),
                );
                return;
            }
            SituacaoDaNota::NaFila => {
                ui.add(Rotulo::apoio("A NF-e está na fila da SEFAZ."));
                return;
            }
            SituacaoDaNota::Rejeitada(motivo) => {
                ui.add(
                    Rotulo::interface(format!("NF-e rejeitada: {motivo}"))
                        .quebravel()
                        .cor(ui.cores().negativo),
                );
                ui.add_space(Espaco::E4);
                if ui
                    .add(Botao::secundario("Emitir NF-e de novo").pequeno())
                    .clicked()
                {
                    pedido = Some(Pedido::EmitirDeNovo(t.id));
                }
                ui.add_space(Espaco::E8);
            }
            SituacaoDaNota::SemNota | SituacaoDaNota::Cancelada => {}
        }
        ui.add(
            Campo::novo("Motivo do cancelamento", &mut estado.filiais.motivo)
                .marcador("ex.: mandei o produto errado"),
        );
        ui.add_space(Espaco::E4);
        if ui
            .add(Botao::destrutivo("Cancelar envio").pequeno())
            .clicked()
        {
            pedido = Some(Pedido::Cancelar(t.id, estado.filiais.motivo.clone()));
        }
    });
    pedido
}
