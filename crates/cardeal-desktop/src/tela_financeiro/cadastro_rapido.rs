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
        } => "Novo fornecedor",
        DlgRapido::Pessoa { .. } => "Novo cliente",
        DlgRapido::Categoria { .. } => "Nova categoria",
    };

    let fechar = Dialogo::nova(titulo).largura(420.0).mostrar(
        ctx,
        estado,
        |ui, estado| {
            let Some(rapido) = &mut estado.dlg_rapido else {
                return;
            };
            match rapido {
                DlgRapido::Pessoa { tipo, nome, .. } => {
                    ui.horizontal(|ui| {
                        for (rot, t) in [
                            ("Pessoa física", TipoPessoa::Fisica),
                            ("Pessoa jurídica", TipoPessoa::Juridica),
                        ] {
                            let sel = *tipo == t;
                            let b = if sel {
                                Botao::primario(rot)
                            } else {
                                Botao::fantasma(rot)
                            };
                            if ui.add(b).clicked() {
                                *tipo = t;
                            }
                        }
                    });
                    ui.add_space(Espaco::E12);
                    ui.add(Campo::novo("Nome", nome).marcador("Nome completo ou razão social"));
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
            tipo,
            nome,
        } => {
            let nome = nome.trim().to_owned();
            if nome.is_empty() {
                notificar(ctx, Notificacao::aviso("Informe o nome."));
                return;
            }
            let (alvo, papel, tipo) = (*alvo, *papel, *tipo);
            let r = motor
                .executar(
                    sessao,
                    "clientes.criar_pessoa.v1",
                    &CriarPessoa {
                        tipo,
                        nome,
                        nome_fantasia: None,
                        papel_inicial: papel,
                        documento_tipo: None,
                        documento_numero: None,
                        data_nascimento: None,
                        endereco: None,
                        contato: None,
                    },
                )
                .map(|p: PessoaCadastrada| p.pessoa);
            match r {
                Ok(id) => {
                    estado.dlg_rapido = None;
                    estado.carregar(motor, sessao);
                    match (alvo, &mut estado.dlg) {
                        (AlvoRapido::Lancar, Dlg::Lancar(f)) => f.contraparte = Some(id),
                        (AlvoRapido::Recorrencia, Dlg::NovaRecorrencia(f)) => {
                            f.contraparte = Some(id);
                        }
                        _ => {}
                    }
                    notificar(
                        ctx,
                        Notificacao::sucesso(if papel == Papel::Fornecedor {
                            "Fornecedor cadastrado"
                        } else {
                            "Cliente cadastrado"
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
