//! Cadastro rápido (cliente/fornecedor, categoria) sem sair do formulário que o pediu.

use super::*;

/// Um botão pequeno e discreto — sem preenchimento nem contorno em repouso, na cor da marca
/// — para abrir um cadastro rápido ([`DlgRapido`]) logo abaixo do seletor que ele
/// complementa.
pub(super) fn botao_cadastro_rapido(rotulo: &str) -> Botao {
    Botao::fantasma(rotulo).pequeno().cor(Rubro::R500)
}

/// O cadastro rápido — um segundo `Dialogo`, menor, empilhado por cima do `dlg` principal
/// (que continua aberto por trás). Ver [`DlgRapido`].
pub(super) fn dialogo_rapido(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaFinanceiro,
) {
    let Some(rapido) = &estado.dlg_rapido else {
        return;
    };
    let titulo = match rapido {
        DlgRapido::Pessoa {
            papel: Papel::Fornecedor,
            ..
        } => "Novo favorecido (fornecedor, locador, sócio…)",
        DlgRapido::Pessoa { .. } => "Novo cliente",
        DlgRapido::Categoria { .. } => "Nova categoria",
    };

    let fechar = Dialogo::nova(titulo).largura(520.0).mostrar(
        ctx,
        estado,
        |ui, estado| {
            let Some(rapido) = &mut estado.dlg_rapido else {
                return;
            };
            match rapido {
                DlgRapido::Pessoa { papel, pessoa, .. } => {
                    let catalogo = if *papel == Papel::Fornecedor {
                        &estado.fornecedores
                    } else {
                        &estado.clientes
                    };
                    pessoa.mostrar_cadastro(ui, catalogo, *papel, false);
                }
                DlgRapido::Categoria { nome, especie, .. } => {
                    ui.add(Campo::novo("Nome", nome).marcador("Aluguel"));
                    ui.add_space(Espaco::E12);
                    ui.horizontal(|ui| {
                        ui.add(Rotulo::campo("Serve para"));
                        for (rot, valor) in [
                            ("Ambas", None),
                            ("Só a receber", Some(EspecieTitulo::Receber)),
                            ("Só a pagar", Some(EspecieTitulo::Pagar)),
                        ] {
                            let sel = *especie == valor;
                            let b = if sel {
                                Botao::primario(rot)
                            } else {
                                Botao::fantasma(rot)
                            };
                            if ui.add(b).clicked() {
                                *especie = valor;
                            }
                        }
                    });
                }
            }
        },
        |ui, estado| {
            if ui.add(Botao::primario("Criar")).clicked() {
                criar_rapido(ui.ctx(), motor, sessao, estado);
            }
            if ui.add(Botao::secundario("Cancelar")).clicked() {
                estado.dlg_rapido = None;
            }
        },
    );
    if fechar {
        estado.dlg_rapido = None;
        return;
    }
    // Clicou "Usar Fulano" no aviso de duplicado: devolve o existente sem esperar "Criar".
    if matches!(&estado.dlg_rapido, Some(DlgRapido::Pessoa { pessoa, .. }) if !pessoa.novo) {
        criar_rapido(ctx, motor, sessao, estado);
    }
}

/// Cria a pessoa/categoria do cadastro rápido e escreve o `Id` de volta no formulário
/// principal ([`AlvoRapido`]) que pediu o cadastro.
pub(super) fn criar_rapido(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaFinanceiro,
) {
    let Some(rapido) = &estado.dlg_rapido else {
        return;
    };
    match rapido {
        DlgRapido::Pessoa {
            alvo,
            papel,
            pessoa,
        } => {
            let (alvo, papel) = (*alvo, *papel);
            // "Usar Fulano" no aviso de duplicado: nada a cadastrar, só devolver o existente.
            let r = if pessoa.novo {
                match pessoa.para_criar(papel, None) {
                    Ok(cmd) => motor
                        .executar(sessao, "clientes.criar_pessoa.v1", &cmd)
                        .map(|p: PessoaCadastrada| (p.pessoa, true)),
                    Err(msg) => {
                        notificar(ctx, Notificacao::aviso(msg));
                        return;
                    }
                }
            } else if let Some(id) = pessoa.selecionada {
                Ok((id, false))
            } else {
                return;
            };
            match r {
                Ok((id, criada)) => {
                    estado.dlg_rapido = None;
                    if criada {
                        estado.carregar(motor, sessao);
                    }
                    match (alvo, &mut estado.dlg) {
                        (AlvoRapido::Lancar, Dlg::Lancar(f)) => {
                            f.pessoa.novo = false;
                            f.pessoa.selecionada = Some(id);
                        }
                        (AlvoRapido::Recorrencia, Dlg::NovaRecorrencia(f)) => {
                            f.contraparte = Some(id);
                        }
                        _ => {}
                    }
                    let quem = if papel == Papel::Fornecedor {
                        "Favorecido"
                    } else {
                        "Cliente"
                    };
                    notificar(
                        ctx,
                        Notificacao::sucesso(if criada {
                            format!("{quem} cadastrado")
                        } else {
                            format!("{quem} já cadastrado — usando o existente")
                        }),
                    );
                }
                Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
            }
        }
        DlgRapido::Categoria {
            alvo,
            nome,
            especie,
        } => {
            let nome = nome.trim().to_owned();
            if nome.is_empty() {
                notificar(ctx, Notificacao::aviso("Informe o nome da categoria."));
                return;
            }
            let (alvo, especie) = (*alvo, *especie);
            let r = motor
                .executar(
                    sessao,
                    "financeiro.criar_categoria.v1",
                    &CriarCategoria { nome, especie },
                )
                .map(|c: mod_financeiro::CategoriaCriada| c.categoria);
            match r {
                Ok(id) => {
                    estado.dlg_rapido = None;
                    estado.carregar_categorias(motor, sessao);
                    match (alvo, &mut estado.dlg) {
                        (AlvoRapido::Lancar, Dlg::Lancar(f)) => f.categoria = Some(id),
                        (AlvoRapido::Recorrencia, Dlg::NovaRecorrencia(f)) => {
                            f.categoria = Some(id);
                        }
                        _ => {}
                    }
                    notificar(ctx, Notificacao::sucesso("Categoria criada"));
                }
                Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
            }
        }
    }
}
