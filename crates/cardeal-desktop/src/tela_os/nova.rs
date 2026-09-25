//! Abrir uma OS (cliente existente ou novo, numa transação só) e corrigir os dados dela.

use super::*;

pub(super) fn dialogo_editar_dados(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaOs,
    ordem_servico: Id,
) {
    let enter =
        ctx.input(|i| i.key_pressed(egui::Key::Enter)) && !ctx.memory(|m| m.any_popup_open());

    let fechar = Dialogo::nova("Editar dados da ordem")
        .largura(560.0)
        .mostrar(
            ctx,
            estado,
            |ui, estado| {
                ui.add(Campo::novo("Aparelho", &mut estado.editar_equipamento));
                ui.add_space(Espaco::E8);
                ui.add(
                    Campo::novo(
                        "Complementar defeito relatado (opcional)",
                        &mut estado.editar_complemento_defeito,
                    )
                    .marcador("acrescenta ao relato original, não substitui"),
                );
                ui.add_space(Espaco::E12);
                estado.editar_ficha.mostrar(ui);
            },
            |ui, estado| {
                let clicou = ui.add(Botao::primario("Salvar")).clicked();
                if clicou || enter {
                    let equipamento = (!estado.editar_equipamento.trim().is_empty())
                        .then(|| estado.editar_equipamento.clone());
                    let complemento = (!estado.editar_complemento_defeito.trim().is_empty())
                        .then(|| estado.editar_complemento_defeito.clone());
                    let ficha = match estado.editar_ficha.validada() {
                        Ok(f) => f,
                        Err(msg) => {
                            notificar(ui.ctx(), Notificacao::aviso(msg));
                            return;
                        }
                    };
                    match motor.executar(
                        sessao,
                        "os.editar_dados_da_ordem.v1",
                        &EditarDadosDaOrdem {
                            ordem_servico,
                            equipamento,
                            complemento_defeito_relatado: complemento,
                            ficha: Some(ficha),
                        },
                    ) {
                        Ok(()) => {
                            estado.dlg = Dlg::Fechado;
                            estado.recarregar_lista(motor, sessao);
                            notificar(ui.ctx(), Notificacao::sucesso("Ordem atualizada"));
                        }
                        Err(e) => notificar(ui.ctx(), Notificacao::erro(e.mensagem)),
                    }
                }
                if ui.add(Botao::secundario("Cancelar")).clicked() {
                    estado.dlg = Dlg::Fechado;
                }
            },
        );
    if fechar {
        estado.dlg = Dlg::Fechado;
    }
}

pub(super) fn dialogo_nova(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaOs,
) {
    let opcoes_cliente: Vec<OpcaoBusca<Id>> = estado
        .clientes
        .iter()
        .map(|c| {
            let opcao = OpcaoBusca::nova(c.pessoa, c.nome.clone());
            match &c.documento {
                Some(doc) => opcao.subtitulo(doc.clone()),
                None => opcao,
            }
        })
        .collect();
    // Enter confirma "Abrir OS" — o mesmo botão que já valida (`abrir_os` mostra o aviso
    // certo se faltar cliente/defeito). Não dispara dentro de um popup aberto (ex.: o combo
    // de UF do endereço).
    let enter =
        ctx.input(|i| i.key_pressed(egui::Key::Enter)) && !ctx.memory(|m| m.any_popup_open());

    let fechar = Dialogo::nova("Nova ordem de serviço")
        .largura(640.0)
        .mostrar(
            ctx,
            estado,
            |ui, estado| {
                let Dlg::Nova {
                    cliente_novo,
                    cliente_sel,
                    cliente_busca,
                    nome,
                    documento,
                    telefone,
                    email,
                    end_logradouro,
                    end_numero,
                    end_bairro,
                    end_cidade,
                    end_uf,
                    end_cep,
                    equipamento,
                    defeito_relatado,
                    ficha,
                } = &mut estado.dlg
                else {
                    return;
                };

                ui.horizontal(|ui| {
                    let sel_existente = if *cliente_novo {
                        Botao::fantasma("Cliente existente")
                    } else {
                        Botao::primario("Cliente existente")
                    };
                    if ui.add(sel_existente).clicked() {
                        *cliente_novo = false;
                    }
                    let sel_novo = if *cliente_novo {
                        Botao::primario("Novo cliente")
                    } else {
                        Botao::fantasma("Novo cliente")
                    };
                    if ui.add(sel_novo).clicked() {
                        *cliente_novo = true;
                    }
                });
                ui.add_space(Espaco::E12);

                if *cliente_novo {
                    // Só o nome é obrigatório — documento, telefone, e-mail e endereço são
                    // opcionais (pedido do usuário: "de obrigatório só o nome").
                    ui.columns(2, |c| {
                        c[0].add(Campo::novo("Nome do cliente", nome));
                        c[1].add(
                            Campo::novo("Documento (opcional)", documento)
                                .mascara(Mascara::Documento)
                                .marcador("CPF ou CNPJ"),
                        );
                    });
                    ui.add_space(Espaco::E8);
                    ui.columns(2, |c| {
                        c[0].add(Campo::novo("Telefone/WhatsApp (opcional)", telefone));
                        c[1].add(Campo::novo("E-mail (opcional)", email));
                    });
                    ui.add_space(Espaco::E8);
                    // Endereço é o bloco mais raramente preenchido na recepção (o cliente só
                    // quer deixar o aparelho) — fica recolhido para não competir com nome e
                    // defeito relatado, que são o que de fato importa pra abrir rápido.
                    SecaoExpansivel::nova("Endereço (opcional)").mostrar(ui, |ui| {
                        ui.columns(2, |c| {
                            c[0].add(Campo::novo("Logradouro", end_logradouro));
                            c[1].add(Campo::novo("Número", end_numero));
                        });
                        ui.columns(2, |c| {
                            c[0].add(Campo::novo("Bairro", end_bairro));
                            c[1].add(Campo::novo("Cidade", end_cidade));
                        });
                        ui.columns(2, |c| {
                            c[0].add(Campo::novo("UF", end_uf).marcador("MG"));
                            c[1].add(Campo::novo("CEP", end_cep).marcador("00000-000"));
                        });
                    });
                } else {
                    SeletorBusca::novo("Cliente", cliente_busca, cliente_sel)
                        .opcoes(opcoes_cliente)
                        .marcador("Buscar por nome ou documento…")
                        .mostrar(ui);
                }
                ui.add_space(Espaco::E16);
                ui.add(Divisor::novo());
                ui.add_space(Espaco::E12);
                ui.add(
                    Campo::novo(
                        "Defeito relatado — o que o cliente quer resolver",
                        defeito_relatado,
                    )
                    .marcador("ex.: não liga, tela quebrada, não carrega"),
                );
                ui.add_space(Espaco::E8);
                ui.add(
                    Campo::novo("Aparelho — o que o cliente trouxe para reparo", equipamento)
                        .marcador("ex.: Furadeira Bosch GSB 13"),
                );
                ui.add_space(Espaco::E12);
                ficha.mostrar(ui);
            },
            |ui, estado| {
                if ui.add(Botao::primario("Abrir OS")).clicked() || enter {
                    abrir_os(ui.ctx(), motor, sessao, estado);
                }
                if ui.add(Botao::secundario("Cancelar")).clicked() {
                    estado.dlg = Dlg::Fechado;
                }
            },
        );
    if fechar {
        estado.dlg = Dlg::Fechado;
    }
}

pub(super) fn abrir_os(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaOs,
) {
    let Dlg::Nova {
        cliente_novo,
        cliente_sel,
        cliente_busca: _,
        nome,
        documento,
        telefone,
        email,
        end_logradouro,
        end_numero,
        end_bairro,
        end_cidade,
        end_uf,
        end_cep,
        equipamento,
        defeito_relatado,
        ficha,
    } = &estado.dlg
    else {
        return;
    };
    let ficha = match ficha.validada() {
        Ok(f) => f,
        Err(msg) => {
            notificar(ctx, Notificacao::aviso(msg));
            return;
        }
    };
    let (cliente_novo, cliente_sel) = (*cliente_novo, *cliente_sel);
    let nome = nome.clone();
    let documento = documento.clone();
    let telefone = telefone.clone();
    let email = email.clone();
    let end_logradouro = end_logradouro.clone();
    let end_numero = end_numero.clone();
    let end_bairro = end_bairro.clone();
    let end_cidade = end_cidade.clone();
    let end_uf = end_uf.clone();
    let end_cep = end_cep.clone();
    let equipamento = equipamento.clone();
    let defeito_relatado = defeito_relatado.clone();

    if equipamento.trim().is_empty() {
        notificar(
            ctx,
            Notificacao::aviso("Informe o aparelho trazido para reparo."),
        );
        return;
    }
    if defeito_relatado.trim().is_empty() {
        notificar(
            ctx,
            Notificacao::aviso("Informe o defeito relatado pelo cliente."),
        );
        return;
    }

    let tecnico_responsavel = sessao.usuario();
    let resultado = if cliente_novo {
        if nome.trim().is_empty() {
            notificar(ctx, Notificacao::aviso("Informe o nome do cliente."));
            return;
        }
        let digitos_doc: String = documento.chars().filter(char::is_ascii_digit).collect();
        let cnpj = digitos_doc.len() == 14;
        let endereco = (!end_logradouro.trim().is_empty()).then_some(EnderecoInicial {
            tipo: if cnpj {
                TipoEndereco::Comercial
            } else {
                TipoEndereco::Residencial
            },
            logradouro: end_logradouro,
            numero: end_numero,
            complemento: None,
            bairro: end_bairro,
            cidade: end_cidade,
            uf: end_uf,
            cep: end_cep,
        });
        // O WhatsApp é o contato principal; o e-mail, se houver, vai junto como extra.
        let (contato, contatos_extras) = match (telefone.trim(), email.trim()) {
            ("", "") => (None, Vec::new()),
            ("", e) => (
                Some(ContatoInicial {
                    tipo: TipoContato::Email,
                    valor: e.to_owned(),
                }),
                Vec::new(),
            ),
            (t, e) => (
                Some(ContatoInicial {
                    tipo: TipoContato::Whatsapp,
                    valor: t.to_owned(),
                }),
                if e.is_empty() {
                    Vec::new()
                } else {
                    vec![ContatoInicial {
                        tipo: TipoContato::Email,
                        valor: e.to_owned(),
                    }]
                },
            ),
        };
        motor
            .executar(
                sessao,
                "os.abrir_ordem_com_cliente_novo.v1",
                &AbrirOrdemComClienteNovo {
                    cliente: CriarPessoa {
                        tipo: if cnpj {
                            TipoPessoa::Juridica
                        } else {
                            TipoPessoa::Fisica
                        },
                        nome,
                        nome_fantasia: None,
                        papel_inicial: PapelCliente::Cliente,
                        documento_tipo: (!digitos_doc.is_empty()).then_some(if cnpj {
                            TipoDocumento::Cnpj
                        } else {
                            TipoDocumento::Cpf
                        }),
                        documento_numero: (!digitos_doc.is_empty()).then_some(documento),
                        data_nascimento: None,
                        endereco,
                        contato,
                    },
                    contatos_extras,
                    equipamento,
                    defeito_relatado,
                    tecnico_responsavel,
                    garantia_dias: 90,
                    ficha,
                },
            )
            .map(|r: OrdemComClienteNovoAberta| r.ordem)
    } else if let Some(cliente) = cliente_sel {
        motor.executar(
            sessao,
            "os.abrir_ordem_servico.v1",
            &AbrirOrdemServico {
                cliente,
                equipamento,
                defeito_relatado,
                tecnico_responsavel,
                garantia_dias: 90,
                ficha,
            },
        )
    } else {
        notificar(
            ctx,
            Notificacao::aviso("Escolha um cliente da lista ou cadastre um novo."),
        );
        return;
    };

    match resultado {
        Ok(aberta) => {
            let aberta: OrdemServicoAberta = aberta;
            estado.erro = None;
            if cliente_novo {
                estado.carregar_clientes(motor, sessao);
            }
            estado.recarregar_lista(motor, sessao);
            estado.abrir_detalhe(motor, sessao, aberta.ordem_servico);
            notificar(
                ctx,
                Notificacao::sucesso(format!("OS #{} aberta", aberta.numero)),
            );
        }
        Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
    }
}
