//! O que a tela pede ao backend: bipar, adicionar, cancelar, finalizar.

use super::*;

// ── ações (falam com o motor) ────────────────────────────────────────────────

pub(super) fn aplicar(
    acao: Acao,
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaPdv,
) {
    match acao {
        Acao::Bipar => bipar(ctx, motor, sessao, estado),
        Acao::Adicionar(produto) => {
            if adicionar(ctx, motor, sessao, estado, produto, Quantidade::UM) {
                estado.entrada.clear();
            }
            estado.foco_entrada = true;
        }
        Acao::CancelarLinha => match estado.linha_ativa_sel() {
            Some(i) => cancelar_item(ctx, motor, sessao, estado, i),
            None => notificar(
                ctx,
                Notificacao::aviso("Escolha um item com ↑↓ (ou clique nele) para cancelar."),
            ),
        },
        Acao::AbrirDesconto => match estado.linha_ativa_sel() {
            Some(linha) => abrir(
                ctx,
                estado,
                Dlg::Desconto {
                    linha,
                    percentual: String::new(),
                },
            ),
            None => notificar(
                ctx,
                Notificacao::aviso("Escolha um item com ↑↓ (ou clique nele) para dar desconto."),
            ),
        },
        Acao::AbrirPagamento => {
            if estado.ativos().count() == 0 {
                notificar(
                    ctx,
                    Notificacao::aviso("Adicione um item antes de finalizar."),
                );
            } else {
                abrir(ctx, estado, Dlg::Pagamento(EstadoPagamento::nova()));
            }
        }
        Acao::Finalizar => pedir_finalizar(ctx, estado),
        Acao::AbrirConsultaPreco => abrir(
            ctx,
            estado,
            Dlg::ConsultaPreco {
                termo: String::new(),
                resultado: None,
            },
        ),
        Acao::ConsultarPreco => consultar_preco(ctx, motor, sessao, estado),
        Acao::AbrirCliente => abrir(
            ctx,
            estado,
            Dlg::Cliente {
                busca: String::new(),
                sel: 0,
            },
        ),
        Acao::EscolherCliente(escolhido) => {
            identificar_cliente(ctx, motor, sessao, estado, escolhido)
        }
        Acao::AbrirSangria => abrir(
            ctx,
            estado,
            Dlg::Sangria {
                valor: String::new(),
                motivo: String::new(),
            },
        ),
        Acao::AbrirFechamento => {
            if estado.cupom.is_some() {
                notificar(
                    ctx,
                    Notificacao::aviso("Finalize ou cancele a venda antes de fechar o caixa."),
                );
            } else {
                abrir(
                    ctx,
                    estado,
                    Dlg::FecharCaixa {
                        contado: String::new(),
                        motivo: String::new(),
                    },
                );
            }
        }
        Acao::PedirCancelarCupom => {
            if estado.cupom.is_some() {
                abrir(
                    ctx,
                    estado,
                    Dlg::CancelarCupom {
                        motivo: String::new(),
                        supervisor: String::new(),
                        senha: String::new(),
                    },
                );
            } else {
                notificar(ctx, Notificacao::aviso("Não há venda em andamento."));
            }
        }
    }
}

/// `Enter` no campo de bipe: código de barras → busca exata; texto → o resultado destacado.
pub(super) fn bipar(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaPdv,
) {
    estado.atualizar_resultados();
    let interpretada = entrada::interpretar(&estado.entrada);
    let quantidade = match interpretada.quantidade() {
        None => Quantidade::UM,
        Some(q) => match q.parse::<Quantidade>() {
            Ok(q) => q,
            Err(_) => {
                notificar(ctx, Notificacao::aviso(format!("Quantidade inválida: {q}")));
                return;
            }
        },
    };

    let produto = match interpretada {
        Entrada::Vazia => return,
        Entrada::Codigo { gtin, .. } => {
            let Some(id) = produto_por_gtin(ctx, motor, sessao, &gtin) else {
                return;
            };
            id
        }
        Entrada::Busca { .. } => {
            let escolhido = estado
                .resultados
                .1
                .get(estado.sel_resultado)
                .and_then(|&i| estado.produtos.get(i))
                .map(|p| p.produto);
            let Some(id) = escolhido else {
                notificar(ctx, Notificacao::aviso("Nenhum produto encontrado."));
                return;
            };
            id
        }
    };

    if adicionar(ctx, motor, sessao, estado, produto, quantidade) {
        estado.entrada.clear();
        estado.sel_resultado = 0;
    }
}

/// Identifica (ou remove) o cliente da venda. Com o cupom já aberto, o backend registra;
/// antes do primeiro item, só guarda — `garantir_cupom` o leva no `AbrirCupom`.
pub(super) fn identificar_cliente(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaPdv,
    escolhido: Option<(Id, String)>,
) {
    if let Some(cupom) = estado.cupom {
        if let Err(e) = motor.executar(
            sessao,
            "pdv.identificar_cliente.v1",
            &IdentificarCliente {
                cupom,
                cliente: escolhido.as_ref().map(|c| c.0),
            },
        ) {
            notificar(ctx, Notificacao::erro(e.mensagem));
            return;
        }
    }
    let aviso = match &escolhido {
        Some((_, nome)) => Notificacao::sucesso(format!("Cliente: {nome}")),
        None => Notificacao::info("Venda sem cliente identificado"),
    };
    notificar(ctx, aviso);
    estado.cliente = escolhido;
    estado.dlg = Dlg::Fechado;
    estado.foco_entrada = true;
}

/// Resolve um código de barras no produto, avisando o operador se não achar.
pub(super) fn produto_por_gtin(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    gtin: &str,
) -> Option<Id> {
    match motor.consultar(
        sessao,
        "estoque.produto_por_codigo_barras.v1",
        &ProdutoPorCodigoBarras {
            codigo_barras: gtin.to_owned(),
        },
    ) {
        Ok(Some(p)) => Some(p.id),
        Ok(None) => {
            notificar(
                ctx,
                Notificacao::aviso(format!("Código {gtin} não cadastrado.")),
            );
            None
        }
        Err(e) => {
            notificar(ctx, Notificacao::erro(e.mensagem));
            None
        }
    }
}

/// `Enter` no diálogo de `F10`: acha o produto (código ou nome) e pergunta ao backend o preço
/// vigente na tabela do terminal. Não mexe no cupom.
pub(super) fn consultar_preco(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaPdv,
) {
    let Dlg::ConsultaPreco { termo, .. } = &estado.dlg else {
        return;
    };
    let interpretada = entrada::interpretar(termo);
    let quantidade = match interpretada.quantidade().map(str::parse::<Quantidade>) {
        None => Quantidade::UM,
        Some(Ok(q)) => q,
        Some(Err(_)) => {
            notificar(ctx, Notificacao::aviso("Quantidade inválida."));
            return;
        }
    };
    let produto = match &interpretada {
        Entrada::Vazia => return,
        Entrada::Codigo { gtin, .. } => produto_por_gtin(ctx, motor, sessao, gtin),
        Entrada::Busca { termo, .. } => {
            let achado = estado
                .produtos
                .iter()
                .find(|p| casa_por_palavras(&p.nome, termo))
                .map(|p| p.produto);
            if achado.is_none() {
                notificar(ctx, Notificacao::aviso("Nenhum produto encontrado."));
            }
            achado
        }
    };
    let (Some(produto), Some(tabela_preco)) = (produto, estado.tabela_sel) else {
        return;
    };
    match motor.consultar(
        sessao,
        "pdv.preco_do_produto.v1",
        &PrecoDoProduto {
            tabela_preco,
            produto,
            quantidade,
        },
    ) {
        Ok(r) => {
            if let Dlg::ConsultaPreco { termo, resultado } = &mut estado.dlg {
                termo.clear();
                *resultado = Some(r);
            }
        }
        Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
    }
}

pub(super) fn garantir_cupom(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaPdv,
) -> Option<Id> {
    if let Some(c) = estado.cupom {
        return Some(c);
    }
    let (Some(sessao_caixa), Some(tabela_preco), Some(local_expedicao)) =
        (estado.sessao_aberta(), estado.tabela_sel, estado.local_sel)
    else {
        notificar(
            ctx,
            Notificacao::aviso("Configure tabela de preço e local antes de vender."),
        );
        return None;
    };
    match motor.executar(
        sessao,
        "pdv.abrir_cupom.v1",
        &AbrirCupom {
            sessao_caixa,
            terminal: estado.terminal,
            serie_fiscal: SERIE_FISCAL,
            cliente: estado.cliente.as_ref().map(|c| c.0),
            tabela_preco,
            local_expedicao,
        },
    ) {
        Ok(CupomAberto {
            cupom,
            numero_terminal,
        }) => {
            estado.cupom = Some(cupom);
            estado.cupom_numero = Some(numero_terminal);
            estado.ultima_venda = None;
            Some(cupom)
        }
        Err(e) => {
            notificar(ctx, Notificacao::erro(e.mensagem));
            None
        }
    }
}

/// Adiciona um item ao cupom. Devolve se deu certo.
pub(super) fn adicionar(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaPdv,
    produto: Id,
    quantidade: Quantidade,
) -> bool {
    let Some(cupom) = garantir_cupom(ctx, motor, sessao, estado) else {
        return false;
    };
    let (nome, codigo) = estado
        .produtos
        .iter()
        .find(|p| p.produto == produto)
        .map_or_else(
            || ("Produto".to_owned(), None),
            |p| (p.nome.clone(), p.codigo_barras.clone()),
        );

    match motor.executar(
        sessao,
        "pdv.adicionar_item.v1",
        &AdicionarItem {
            cupom,
            produto,
            variacao: None,
            quantidade,
        },
    ) {
        Ok(ItemFoiAdicionado {
            item,
            preco_unitario,
            total_cupom,
        }) => {
            let total = Dinheiro::de_total(quantidade, preco_unitario, Arredondamento::MeioAcima);
            estado.carrinho.push(Linha {
                item,
                nome,
                codigo,
                quantidade,
                preco: preco_unitario,
                desconto: Percentual::ZERO,
                total,
                cancelado: false,
            });
            estado.linha_sel = Some(estado.carrinho.len() - 1);
            estado.total = total_cupom;
            estado.erro = None;
            true
        }
        Err(e) => {
            notificar(ctx, Notificacao::erro(e.mensagem));
            false
        }
    }
}

pub(super) fn cancelar_item(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaPdv,
    i: usize,
) {
    let (Some(cupom), Some(linha)) = (estado.cupom, estado.carrinho.get(i)) else {
        return;
    };
    let item = linha.item;
    match motor.executar(
        sessao,
        "pdv.cancelar_item.v1",
        &mod_pdv::CancelarItem {
            cupom,
            item,
            motivo: "Removido no balcão".to_owned(),
        },
    ) {
        Ok(()) => {
            if let Some(l) = estado.carrinho.get_mut(i) {
                l.cancelado = true;
            }
            estado.recalcular_total();
            estado.linha_sel = None;
            estado.mover_linha(true);
            estado.erro = None;
        }
        Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
    }
}

/// Cancela o cupom em nome de quem o **autorizou** (o supervisor): é a sessão dele que executa
/// o comando e que fica em `autorizado_por` — um operador de caixa, em geral, não tem a
/// permissão `pdv.cupom.cancelar`.
pub(super) fn cancelar_cupom(
    ctx: &egui::Context,
    motor: &MotorLocal,
    autorizador: &SessaoLocal,
    estado: &mut EstadoTelaPdv,
    motivo: String,
) -> bool {
    let Some(cupom) = estado.cupom else {
        return false;
    };
    match motor.executar(
        autorizador,
        "pdv.cancelar_cupom.v1",
        &CancelarCupom {
            cupom,
            motivo,
            autorizado_por: autorizador.usuario(),
        },
    ) {
        Ok(()) => {
            estado.cupom = None;
            estado.cupom_numero = None;
            estado.cliente = None;
            estado.carrinho.clear();
            estado.linha_sel = None;
            estado.total = Dinheiro::ZERO;
            estado.erro = None;
            estado.foco_entrada = true;
            notificar(ctx, Notificacao::info("Cupom cancelado"));
            true
        }
        Err(e) => {
            notificar(ctx, Notificacao::erro(e.mensagem));
            false
        }
    }
}

/// Valida o pagamento e agenda a finalização para o quadro seguinte.
pub(super) fn pedir_finalizar(ctx: &egui::Context, estado: &mut EstadoTelaPdv) {
    let Dlg::Pagamento(p) = &estado.dlg else {
        return;
    };
    let situacao = pagamento::situacao(estado.total, &p.valores);
    let aviso = match situacao {
        Situacao::Falta(d) => Some(format!("Falta receber {}.", d.formatar_com_simbolo())),
        Situacao::Excesso(d) => Some(format!(
            "Passou {} do total e só o dinheiro dá troco.",
            d.formatar_com_simbolo()
        )),
        Situacao::Fecha | Situacao::Troco(_) => None,
    };
    if let Some(a) = aviso {
        notificar(ctx, Notificacao::aviso(a));
        return;
    }
    estado.finalizar_pendente = true;
    notificar(
        ctx,
        Notificacao::carregando("Emitindo NFC-e…").id(egui::Id::new("pdv-fiscal")),
    );
    ctx.request_repaint();
}

pub(super) fn finalizar(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaPdv,
) {
    let fiscal = egui::Id::new("pdv-fiscal");
    let Some(cupom) = estado.cupom else { return };
    let Dlg::Pagamento(p) = &estado.dlg else {
        return;
    };
    let total = estado.total;
    let troco = match pagamento::situacao(total, &p.valores) {
        Situacao::Troco(t) => t,
        _ => Dinheiro::ZERO,
    };
    let pagamentos = pagamento::montar(total, &p.valores);

    match motor.executar(
        sessao,
        "pdv.finalizar_venda.v1",
        &FinalizarVenda { cupom, pagamentos },
    ) {
        Ok(VendaFoiFinalizada { total, .. }) => {
            let numero = estado.cupom_numero.unwrap_or(0);
            estado.ultima_venda = Some(UltimaVenda {
                numero,
                total,
                troco,
            });
            estado.cupom = None;
            estado.cupom_numero = None;
            estado.cliente = None;
            estado.carrinho.clear();
            estado.linha_sel = None;
            estado.total = Dinheiro::ZERO;
            estado.dlg = Dlg::Fechado;
            estado.erro = None;
            estado.foco_entrada = true;
            estado.carregar(motor, sessao);
            let detalhe = if troco > Dinheiro::ZERO {
                format!(
                    "{} · troco {}",
                    total.formatar_com_simbolo(),
                    troco.formatar_com_simbolo()
                )
            } else {
                format!("NFC-e autorizada · {}", total.formatar_com_simbolo())
            };
            notificar(
                ctx,
                Notificacao::sucesso(format!("Venda #{numero} concluída"))
                    .detalhe(detalhe)
                    .id(fiscal),
            );
        }
        Err(e) => notificar(ctx, Notificacao::erro(e.mensagem).id(fiscal)),
    }
}
