//! Tela de Clientes — lista + dialog (criar / ver / editar). `docs/modulos/clientes.md`.
//!
//! É o caso de cadastro puro: o dialog tem os três modos da regra de UI do projeto —
//! **Criar** (campos vazios), **Ver** (campos como texto + botão "Editar"), **Editar**
//! (campos de novo editáveis, Salvar / Cancelar).
//!
//! Cadastro/busca rápida é o objetivo desta tela (`docs/12-ui-ux.md`): ela é o primeiro passo
//! de quase toda abertura de OS, então a lista precisa escanear bem (busca visível, badge de
//! "sem documento", ordenação por coluna) e o dialog "Ver" precisa trazer telefone/endereço —
//! já cadastrados no backend (`PessoaDetalhada`), só não apareciam na tela.

use cardeal_cliente::{MotorLocal, SessaoLocal};
use cardeal_kernel::Id;
use cardeal_modkit::Icone;
use cardeal_ui::atoms::{Botao, Etiqueta, Rotulo};
use cardeal_ui::molecules::{
    Campo, CartaoKpi, EstadoVazio, Mascara, SecaoExpansivel, SeletorOpcao,
};
use cardeal_ui::organisms::{
    notificar, ColunaGrade, Dialogo, Direcao, FaixaKpi, Grade, LayoutTela, Notificacao,
};
use cardeal_ui::tokens::{Espaco, TemaUi};
use eframe::egui;
use mod_clientes::{
    AdicionarContato, AdicionarEndereco, ContatoInicial, CriarPessoa, DesativarPessoa,
    DetalhePessoa, EditarPessoa, EnderecoInicial, ItemPessoa, Papel, PessoaCadastrada,
    PessoaDetalhada, PessoaEditada, PessoasPorPapel, TipoContato, TipoDocumento, TipoEndereco,
    TipoPessoa,
};

#[derive(Clone, Copy, PartialEq)]
enum Modo {
    Criar,
    Ver,
    Editar,
}

struct Form {
    modo: Modo,
    pessoa: Option<Id>,
    tipo: Option<TipoPessoa>,
    nome: String,
    nome_fantasia: String,
    documento: String,
    observacao: String,
    /// `true` depois de um clique em "Cadastrar"/"Salvar" — só a partir daí o campo Nome
    /// vazio aparece com o contorno de erro (não já de cara, num formulário recém-aberto).
    tentou_salvar: bool,
    // Só leitura (modo Ver):
    papeis: String,
    limite: String,
    // Editáveis em Criar e Editar (mostrados também, só leitura, em Ver):
    telefone: String,
    email: String,
    end_logradouro: String,
    end_numero: String,
    end_complemento: String,
    end_bairro: String,
    end_cidade: String,
    end_uf: String,
    end_cep: String,
    // Snapshot do que veio do backend, pra `salvar` só gravar um novo contato/endereço
    // quando o atendente de fato mudou algo (evita empilhar uma linha nova a cada "Salvar").
    telefone_original: String,
    email_original: String,
    endereco_original: String,
}

impl Form {
    fn novo() -> Self {
        Self {
            modo: Modo::Criar,
            pessoa: None,
            tipo: Some(TipoPessoa::Fisica),
            nome: String::new(),
            nome_fantasia: String::new(),
            documento: String::new(),
            observacao: String::new(),
            tentou_salvar: false,
            papeis: String::new(),
            limite: String::new(),
            telefone: String::new(),
            email: String::new(),
            end_logradouro: String::new(),
            end_numero: String::new(),
            end_complemento: String::new(),
            end_bairro: String::new(),
            end_cidade: String::new(),
            end_uf: String::new(),
            end_cep: String::new(),
            telefone_original: String::new(),
            email_original: String::new(),
            endereco_original: String::new(),
        }
    }

    fn de_detalhe(d: &PessoaDetalhada) -> Self {
        let p = &d.pessoa;
        let papeis = p
            .papeis
            .iter()
            .filter(|pp| pp.ativo)
            .map(|pp| rotulo_papel(pp.papel))
            .collect::<Vec<_>>()
            .join(", ");
        let telefone = d
            .contatos
            .iter()
            .find(|c| {
                c.principal
                    && matches!(
                        c.tipo,
                        TipoContato::Whatsapp | TipoContato::Celular | TipoContato::Telefone
                    )
            })
            .or_else(|| {
                d.contatos.iter().find(|c| {
                    matches!(
                        c.tipo,
                        TipoContato::Whatsapp | TipoContato::Celular | TipoContato::Telefone
                    )
                })
            })
            .map(|c| formatar_telefone(&c.valor))
            .unwrap_or_default();
        let email = d
            .contatos
            .iter()
            .find(|c| c.tipo == TipoContato::Email)
            .map(|c| c.valor.clone())
            .unwrap_or_default();
        let end = d.enderecos.first();
        let end_logradouro = end.map_or_else(String::new, |e| e.logradouro.clone());
        let end_numero = end.map_or_else(String::new, |e| e.numero.clone());
        let end_complemento = end.and_then(|e| e.complemento.clone()).unwrap_or_default();
        let end_bairro = end.map_or_else(String::new, |e| e.bairro.clone());
        let end_cidade = end.map_or_else(String::new, |e| e.cidade.clone());
        let end_uf = end.map_or_else(String::new, |e| e.uf.sigla().to_owned());
        let end_cep = end.map_or_else(String::new, |e| e.cep.clone());
        let endereco_original = assinatura_endereco(
            &end_logradouro,
            &end_numero,
            &end_complemento,
            &end_bairro,
            &end_cidade,
            &end_uf,
            &end_cep,
        );
        Self {
            modo: Modo::Ver,
            pessoa: Some(p.id),
            tipo: Some(p.tipo),
            nome: p.nome.clone(),
            nome_fantasia: p.nome_fantasia.clone().unwrap_or_default(),
            documento: d
                .documentos
                .first()
                .map(|doc| doc.numero.clone())
                .unwrap_or_default(),
            observacao: p.observacao.clone().unwrap_or_default(),
            tentou_salvar: false,
            papeis,
            limite: d
                .limite_credito
                .as_ref()
                .map(|l| l.limite.formatar_com_simbolo())
                .unwrap_or_default(),
            telefone: telefone.clone(),
            email: email.clone(),
            end_logradouro,
            end_numero,
            end_complemento,
            end_bairro,
            end_cidade,
            end_uf,
            end_cep,
            telefone_original: telefone,
            email_original: email,
            endereco_original,
        }
    }
}

/// Uma chave de comparação barata pra saber se o endereço do formulário mudou desde que foi
/// carregado — evita `AdicionarEndereco` gravar uma linha nova a cada "Salvar" quando o
/// atendente só trocou a observação, por exemplo.
fn assinatura_endereco(
    logradouro: &str,
    numero: &str,
    complemento: &str,
    bairro: &str,
    cidade: &str,
    uf: &str,
    cep: &str,
) -> String {
    format!("{logradouro}|{numero}|{complemento}|{bairro}|{cidade}|{uf}|{cep}")
}

const fn rotulo_papel(p: Papel) -> &'static str {
    match p {
        Papel::Cliente => "Cliente",
        Papel::Fornecedor => "Fornecedor",
        Papel::Transportadora => "Transportadora",
        Papel::Funcionario => "Funcionário",
        Papel::Socio => "Sócio",
        Papel::Vendedor => "Vendedor",
    }
}

/// `(11) 91234-5678` a partir de dígitos crus (10 ou 11 dígitos, com DDD) — o mesmo padrão de
/// `formatar_documento` em `cardeal_ui::molecules::campo`, só que não existe um `Mascara`
/// pronto para telefone ainda porque só esta tela precisava até agora.
fn formatar_telefone(digitos: &str) -> String {
    match digitos.len() {
        11 => format!(
            "({}) {}-{}",
            &digitos[0..2],
            &digitos[2..7],
            &digitos[7..11]
        ),
        10 => format!(
            "({}) {}-{}",
            &digitos[0..2],
            &digitos[2..6],
            &digitos[6..10]
        ),
        _ => digitos.to_owned(),
    }
}

/// Estado local da tela.
#[derive(Default)]
pub struct EstadoTelaClientes {
    pessoas: Vec<ItemPessoa>,
    busca: String,
    erro: Option<String>,
    form: Option<Form>,
    ordenacao: Option<(usize, Direcao)>,
    /// `(pessoa, nome)` do cliente aguardando confirmação de exclusão — desenha o dialog de
    /// confirmação num frame separado do clique no botão "Excluir" (o padrão já usado por
    /// `tela_os.rs` pra cancelamento).
    confirmar_exclusao: Option<(Id, String)>,
}

impl EstadoTelaClientes {
    /// Recarrega a lista de clientes (aplicando o termo de busca atual).
    pub fn carregar(&mut self, motor: &MotorLocal, sessao: &SessaoLocal) {
        let termo = (!self.busca.trim().is_empty()).then(|| self.busca.clone());
        match motor.consultar(
            sessao,
            "clientes.pessoas_por_papel.v1",
            &PessoasPorPapel {
                papel: Papel::Cliente,
                busca: termo,
            },
        ) {
            Ok(p) => {
                self.pessoas = p;
                self.erro = None;
                if let Some((coluna, direcao)) = self.ordenacao {
                    ordenar_pessoas(&mut self.pessoas, coluna, direcao);
                }
            }
            Err(e) => self.erro = Some(e.mensagem),
        }
    }

    fn abrir_detalhe(&mut self, motor: &MotorLocal, sessao: &SessaoLocal, id: Id) {
        match motor.consultar(
            sessao,
            "clientes.detalhe_pessoa.v1",
            &DetalhePessoa { pessoa: id },
        ) {
            Ok(Some(d)) => self.form = Some(Form::de_detalhe(&d)),
            Ok(None) => self.erro = Some("Pessoa não encontrada.".to_owned()),
            Err(e) => self.erro = Some(e.mensagem),
        }
    }
}

/// Desenha a tela inteira.
pub fn mostrar(
    ui: &mut egui::Ui,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaClientes,
) {
    LayoutTela::nova("Clientes").mostrar(
        ui,
        estado,
        |ui, estado| {
            if ui
                .add(Botao::primario("+ Novo cliente").atalho("Ctrl+N"))
                .clicked()
            {
                estado.form = Some(Form::novo());
            }
        },
        |ui, estado| {
            if !estado.pessoas.is_empty() {
                let sem_documento = estado
                    .pessoas
                    .iter()
                    .filter(|p| p.documento.is_none())
                    .count();
                FaixaKpi::nova(vec![
                    CartaoKpi::contagem("Clientes cadastrados", estado.pessoas.len()),
                    CartaoKpi::contagem("Sem documento", sem_documento)
                        .variacao("CPF/CNPJ ainda não informado"),
                ])
                .mostrar(ui);
            }

            ui.horizontal(|ui| {
                ui.set_max_width(360.0);
                if ui
                    .add(
                        cardeal_ui::molecules::Campo::novo("", &mut estado.busca)
                            .marcador("Buscar por nome ou documento"),
                    )
                    .changed()
                {
                    estado.carregar(motor, sessao);
                }
                if !estado.busca.is_empty() && ui.add(Botao::fantasma("✕").pequeno()).clicked() {
                    estado.busca.clear();
                    estado.carregar(motor, sessao);
                }
            });
            ui.add_space(Espaco::E12);

            if let Some(erro) = &estado.erro {
                ui.add(
                    Rotulo::interface(erro.clone())
                        .quebravel()
                        .cor(ui.cores().negativo),
                );
                ui.add_space(Espaco::E12);
            }
            lista(ui, motor, sessao, estado);
        },
    );

    if estado.form.is_some() {
        dialogo(ui.ctx(), motor, sessao, estado);
    }

    if let Some((pessoa, nome)) = estado.confirmar_exclusao.clone() {
        let resposta = cardeal_ui::organisms::dialogo_confirmacao(
            ui.ctx(),
            "Excluir cliente",
            &format!(
                "Tem certeza que quer excluir \"{nome}\"? O cadastro sai da busca padrão, mas \
                 nada é apagado — dá pra reativar depois, se precisar.",
            ),
            "Excluir",
        );
        if resposta.confirmado {
            excluir(ui.ctx(), motor, sessao, estado, pessoa);
        }
        if resposta.fechar {
            estado.confirmar_exclusao = None;
        }
    }
}

fn lista(
    ui: &mut egui::Ui,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaClientes,
) {
    if estado.pessoas.is_empty() {
        let msg = if estado.busca.trim().is_empty() {
            "Nenhum cliente cadastrado ainda."
        } else {
            "Nenhum cliente para essa busca."
        };
        if estado.erro.is_none()
            && EstadoVazio::novo(Icone::Pessoas, msg)
                .acao("Cadastrar cliente")
                .mostrar(ui)
        {
            estado.form = Some(Form::novo());
        }
        return;
    }

    let colunas = vec![
        ColunaGrade::nova("Nome"),
        ColunaGrade::nova("WhatsApp").largura(220.0),
        ColunaGrade::nova("Ações").largura(190.0),
    ];
    let mut editar_clicado = None;
    let mut excluir_clicado = None;
    let resposta = Grade::nova(colunas)
        .selecionavel(None)
        .ordenacao(estado.ordenacao)
        // Linha mais alta que o padrão (38px) — a coluna "Ações" carrega botões de 34px de
        // altura mínima, que ficavam praticamente colados nas bordas da linha sem isso.
        .altura_linha(cardeal_ui::tokens::AlturaLinha::Toque)
        .mostrar(ui, estado.pessoas.len(), |i, row| {
            let p = &estado.pessoas[i];
            row.col(|ui| {
                ui.add(Rotulo::interface(p.nome.clone()));
            });
            row.col(|ui| match &p.telefone {
                Some(tel) => {
                    ui.add(Rotulo::campo(formatar_telefone(tel)));
                }
                None => {
                    ui.add(Etiqueta::atencao("sem WhatsApp"));
                }
            });
            row.col(|ui| {
                let rubro = ui.cores().rubro;
                ui.horizontal(|ui| {
                    if ui.add(Botao::fantasma("Editar").pequeno().cor(rubro)).clicked() {
                        editar_clicado = Some(p.pessoa);
                    }
                    if ui.add(Botao::destrutivo("Excluir").pequeno()).clicked() {
                        excluir_clicado = Some((p.pessoa, p.nome.clone()));
                    }
                });
            });
        });

    if let Some(coluna) = resposta.coluna_clicada {
        let direcao = match estado.ordenacao {
            Some((atual, direcao)) if atual == coluna => direcao.invertida(),
            _ => Direcao::Ascendente,
        };
        estado.ordenacao = Some((coluna, direcao));
        ordenar_pessoas(&mut estado.pessoas, coluna, direcao);
    }
    if let Some(id) = editar_clicado {
        estado.abrir_detalhe(motor, sessao, id);
        if let Some(f) = estado.form.as_mut() {
            f.modo = Modo::Editar;
        }
    } else if let Some(pessoa) = excluir_clicado {
        estado.confirmar_exclusao = Some(pessoa);
    } else if let Some(i) = resposta.linha_clicada {
        let id = estado.pessoas[i].pessoa;
        estado.abrir_detalhe(motor, sessao, id);
    }
}

/// Ordena `pessoas` pela coluna clicada no cabeçalho da [`Grade`] (Nome, WhatsApp).
fn ordenar_pessoas(pessoas: &mut [ItemPessoa], coluna: usize, direcao: Direcao) {
    pessoas.sort_by(|a, b| {
        let ordem = match coluna {
            0 => a.nome.cmp(&b.nome),
            1 => a.telefone.cmp(&b.telefone),
            _ => std::cmp::Ordering::Equal,
        };
        match direcao {
            Direcao::Ascendente => ordem,
            Direcao::Descendente => ordem.reverse(),
        }
    });
}

fn dialogo(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaClientes,
) {
    let modo = estado.form.as_ref().map_or(Modo::Criar, |f| f.modo);
    let titulo = match modo {
        Modo::Criar => "Novo cliente".to_owned(),
        _ => estado
            .form
            .as_ref()
            .map_or_else(String::new, |f| f.nome.clone()),
    };
    // Enter confirma a ação primária do dialog (Cadastrar/Salvar) — nunca em modo "Ver", que
    // não tem o que confirmar, e nunca quando um combo/popup está aberto (o Enter é dele).
    let enter = matches!(modo, Modo::Criar | Modo::Editar)
        && ctx.input(|i| i.key_pressed(egui::Key::Enter))
        && !ctx.memory(|m| m.any_popup_open());

    let fechar = Dialogo::nova(titulo).largura(640.0).mostrar(
        ctx,
        estado,
        |ui, estado| {
            let Some(f) = estado.form.as_mut() else {
                return;
            };
            let leitura = f.modo == Modo::Ver;
            let pj = matches!(f.tipo, Some(TipoPessoa::Juridica));

            if f.modo == Modo::Criar {
                SeletorOpcao::novo("Tipo", &mut f.tipo)
                    .opcao(TipoPessoa::Fisica, "Pessoa física")
                    .opcao(TipoPessoa::Juridica, "Pessoa jurídica")
                    .mostrar(ui);
                ui.add_space(Espaco::E12);
            }

            // Cadastro rápido de assistência técnica: o nome é o único campo que de fato
            // bloqueia o salvamento (ver `criar`, mais abaixo) — documento, nome fantasia e
            // observação são todos opcionais no backend há tempos (`CriarPessoa` já aceita
            // `documento_*: None`), mas a tela nunca avisava isso visualmente, então o
            // atendente hesitava tentando preencher CPF de um cliente que só quer deixar o
            // aparelho e buscar depois. Rotulados explicitamente agora.
            ui.columns(2, |c| {
                let nome_vazio = f.tentou_salvar && f.nome.trim().is_empty();
                c[0].add(
                    Campo::novo(if pj { "Razão social" } else { "Nome" }, &mut f.nome)
                        .somente_leitura(leitura)
                        .erro(nome_vazio.then_some("Obrigatório")),
                );
                c[1].add(
                    Campo::novo(
                        if pj {
                            "CNPJ (opcional)"
                        } else {
                            "CPF (opcional)"
                        },
                        &mut f.documento,
                    )
                    .mascara(Mascara::Documento)
                    .somente_leitura(leitura || f.modo == Modo::Editar),
                );
            });
            ui.add_space(Espaco::E12);

            if pj {
                ui.add(
                    Campo::novo("Nome fantasia (opcional)", &mut f.nome_fantasia)
                        .somente_leitura(leitura),
                );
                ui.add_space(Espaco::E12);
            }

            ui.add(
                Campo::novo("Observação (opcional)", &mut f.observacao).somente_leitura(leitura),
            );
            ui.add_space(Espaco::E12);

            ui.columns(2, |c| {
                c[0].add(
                    Campo::novo("Telefone/WhatsApp (opcional)", &mut f.telefone)
                        .somente_leitura(leitura),
                );
                c[1].add(Campo::novo("E-mail (opcional)", &mut f.email).somente_leitura(leitura));
            });
            ui.add_space(Espaco::E12);

            let tem_endereco = !f.end_logradouro.trim().is_empty();
            SecaoExpansivel::nova("Endereço (opcional)")
                .aberta_por_padrao(tem_endereco)
                .mostrar(ui, |ui| {
                    ui.columns(2, |c| {
                        c[0].add(
                            Campo::novo("Logradouro", &mut f.end_logradouro)
                                .somente_leitura(leitura),
                        );
                        c[1].add(Campo::novo("Número", &mut f.end_numero).somente_leitura(leitura));
                    });
                    ui.columns(2, |c| {
                        c[0].add(
                            Campo::novo("Complemento (opcional)", &mut f.end_complemento)
                                .somente_leitura(leitura),
                        );
                        c[1].add(Campo::novo("Bairro", &mut f.end_bairro).somente_leitura(leitura));
                    });
                    ui.columns(2, |c| {
                        c[0].add(Campo::novo("Cidade", &mut f.end_cidade).somente_leitura(leitura));
                        c[1].add(
                            Campo::novo("UF", &mut f.end_uf)
                                .somente_leitura(leitura)
                                .marcador("MG"),
                        );
                    });
                    ui.add(
                        Campo::novo("CEP", &mut f.end_cep)
                            .somente_leitura(leitura)
                            .marcador("00000-000"),
                    );
                });

            if f.modo == Modo::Ver {
                ui.add_space(Espaco::E16);
                ui.separator();
                ui.add_space(Espaco::E12);
                ui.columns(2, |c| {
                    kv(&mut c[0], "Papéis", &f.papeis);
                    kv(&mut c[1], "Limite de crédito", &f.limite);
                });
            }
        },
        |ui, estado| {
            let modo = estado.form.as_ref().map_or(Modo::Criar, |f| f.modo);
            match modo {
                Modo::Criar => {
                    let clicou = ui.add(Botao::primario("Cadastrar")).clicked();
                    if clicou || enter {
                        if let Some(f) = estado.form.as_mut() {
                            f.tentou_salvar = true;
                        }
                        criar(ui.ctx(), motor, sessao, estado);
                    }
                    if ui.add(Botao::secundario("Cancelar")).clicked() {
                        estado.form = None;
                    }
                }
                Modo::Ver => {
                    if ui.add(Botao::primario("Editar")).clicked() {
                        if let Some(f) = estado.form.as_mut() {
                            f.modo = Modo::Editar;
                        }
                    }
                    if ui.add(Botao::secundario("Fechar")).clicked() {
                        estado.form = None;
                    }
                }
                Modo::Editar => {
                    let clicou = ui.add(Botao::primario("Salvar")).clicked();
                    if clicou || enter {
                        if let Some(f) = estado.form.as_mut() {
                            f.tentou_salvar = true;
                        }
                        salvar(ui.ctx(), motor, sessao, estado);
                    }
                    if ui.add(Botao::secundario("Cancelar")).clicked() {
                        if let Some(f) = estado.form.as_mut() {
                            f.modo = Modo::Ver;
                        }
                    }
                }
            }
        },
    );
    if fechar {
        estado.form = None;
    }
}

fn kv(ui: &mut egui::Ui, chave: &str, valor: &str) {
    ui.add(Rotulo::campo(chave));
    ui.add(Rotulo::interface(if valor.trim().is_empty() {
        "—"
    } else {
        valor
    }));
}

/// Monta o endereço inicial pro formulário, se o logradouro foi preenchido — mesma decisão
/// de `tela_os.rs` (residencial por padrão; a tela não expõe tipo de endereço, cadastro
/// rápido de balcão).
fn montar_endereco_inicial(f: &Form) -> Option<EnderecoInicial> {
    (!f.end_logradouro.trim().is_empty()).then(|| EnderecoInicial {
        tipo: TipoEndereco::Residencial,
        logradouro: f.end_logradouro.clone(),
        numero: f.end_numero.clone(),
        complemento: (!f.end_complemento.trim().is_empty()).then(|| f.end_complemento.clone()),
        bairro: f.end_bairro.clone(),
        cidade: f.end_cidade.clone(),
        uf: f.end_uf.clone(),
        cep: f.end_cep.clone(),
    })
}

fn criar(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaClientes,
) {
    let Some(f) = estado.form.as_ref() else {
        return;
    };
    if f.nome.trim().is_empty() {
        notificar(ctx, Notificacao::aviso("Informe o nome do cliente."));
        return;
    }
    let pj = matches!(f.tipo, Some(TipoPessoa::Juridica));
    let doc = f.documento.trim().to_string();
    let tem_doc = !doc.is_empty();
    let telefone = f.telefone.trim().to_string();
    let email = f.email.trim().to_string();
    // `CriarPessoa` só aceita um contato inicial — quando telefone E e-mail vêm preenchidos,
    // o telefone entra na criação e o e-mail via `AdicionarContato` logo depois (mesmo padrão
    // já usado pela abertura rápida de cliente em `tela_os.rs`).
    let contato_inicial = if !telefone.is_empty() {
        Some(ContatoInicial {
            tipo: TipoContato::Whatsapp,
            valor: telefone.clone(),
        })
    } else if !email.is_empty() {
        Some(ContatoInicial {
            tipo: TipoContato::Email,
            valor: email.clone(),
        })
    } else {
        None
    };
    let cmd = CriarPessoa {
        tipo: f.tipo.unwrap_or(TipoPessoa::Fisica),
        nome: f.nome.clone(),
        nome_fantasia: (pj && !f.nome_fantasia.trim().is_empty()).then(|| f.nome_fantasia.clone()),
        papel_inicial: Papel::Cliente,
        documento_tipo: tem_doc.then_some(if pj {
            TipoDocumento::Cnpj
        } else {
            TipoDocumento::Cpf
        }),
        documento_numero: tem_doc.then(|| doc.clone()),
        data_nascimento: None,
        endereco: montar_endereco_inicial(f),
        contato: contato_inicial,
    };
    match motor.executar(sessao, "clientes.criar_pessoa.v1", &cmd) {
        Ok(r) => {
            let r: PessoaCadastrada = r;
            if !telefone.is_empty() && !email.is_empty() {
                if let Err(e) = motor.executar(
                    sessao,
                    "clientes.adicionar_contato.v1",
                    &AdicionarContato {
                        pessoa: r.pessoa,
                        tipo: TipoContato::Email,
                        valor: email,
                        principal: true,
                    },
                ) {
                    notificar(
                        ctx,
                        Notificacao::aviso("Cliente criado, mas o e-mail não foi salvo")
                            .detalhe(e.mensagem),
                    );
                }
            }
            estado.form = None;
            estado.carregar(motor, sessao);
            notificar(ctx, Notificacao::sucesso("Cliente cadastrado"));
        }
        Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
    }
}

fn salvar(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaClientes,
) {
    let Some(f) = estado.form.as_ref() else {
        return;
    };
    if f.nome.trim().is_empty() {
        notificar(ctx, Notificacao::aviso("Informe o nome do cliente."));
        return;
    }
    let Some(id) = f.pessoa else { return };
    let cmd = EditarPessoa {
        pessoa: id,
        nome: Some(f.nome.clone()),
        nome_fantasia: Some(f.nome_fantasia.clone()).filter(|s| !s.trim().is_empty()),
        observacao: Some(f.observacao.clone()).filter(|s| !s.trim().is_empty()),
        data_nascimento: None,
    };
    match motor.executar(sessao, "clientes.editar_pessoa.v1", &cmd) {
        Ok(r) => {
            let _: PessoaEditada = r;

            // Só grava um contato/endereço novo quando o atendente de fato mudou o valor —
            // `AdicionarContato`/`AdicionarEndereco` são aditivos (nunca sobrescrevem em
            // linha), então chamar sempre empilharia uma linha idêntica a cada "Salvar".
            let telefone = f.telefone.trim().to_string();
            if !telefone.is_empty() && telefone != f.telefone_original.trim() {
                if let Err(e) = motor.executar(
                    sessao,
                    "clientes.adicionar_contato.v1",
                    &AdicionarContato {
                        pessoa: id,
                        tipo: TipoContato::Whatsapp,
                        valor: telefone,
                        principal: true,
                    },
                ) {
                    notificar(
                        ctx,
                        Notificacao::aviso("Cliente atualizado, mas o telefone não foi salvo")
                            .detalhe(e.mensagem),
                    );
                }
            }
            let email = f.email.trim().to_string();
            if !email.is_empty() && email != f.email_original.trim() {
                if let Err(e) = motor.executar(
                    sessao,
                    "clientes.adicionar_contato.v1",
                    &AdicionarContato {
                        pessoa: id,
                        tipo: TipoContato::Email,
                        valor: email,
                        principal: true,
                    },
                ) {
                    notificar(
                        ctx,
                        Notificacao::aviso("Cliente atualizado, mas o e-mail não foi salvo")
                            .detalhe(e.mensagem),
                    );
                }
            }
            let assinatura_atual = assinatura_endereco(
                &f.end_logradouro,
                &f.end_numero,
                &f.end_complemento,
                &f.end_bairro,
                &f.end_cidade,
                &f.end_uf,
                &f.end_cep,
            );
            if !f.end_logradouro.trim().is_empty() && assinatura_atual != f.endereco_original {
                if let Some(endereco) = montar_endereco_inicial(f) {
                    if let Err(e) = motor.executar(
                        sessao,
                        "clientes.adicionar_endereco.v1",
                        &AdicionarEndereco {
                            pessoa: id,
                            tipo: endereco.tipo,
                            logradouro: endereco.logradouro,
                            numero: endereco.numero,
                            complemento: endereco.complemento,
                            bairro: endereco.bairro,
                            cidade: endereco.cidade,
                            uf: endereco.uf,
                            cep: endereco.cep,
                            principal: true,
                        },
                    ) {
                        notificar(
                            ctx,
                            Notificacao::aviso("Cliente atualizado, mas o endereço não foi salvo")
                                .detalhe(e.mensagem),
                        );
                    }
                }
            }

            estado.abrir_detalhe(motor, sessao, id);
            estado.carregar(motor, sessao);
            notificar(ctx, Notificacao::sucesso("Cliente atualizado"));
        }
        Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
    }
}

fn excluir(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaClientes,
    pessoa: Id,
) {
    match motor.executar(
        sessao,
        "clientes.desativar_pessoa.v1",
        &DesativarPessoa { pessoa },
    ) {
        Ok(r) => {
            let _: mod_clientes::PessoaDesativada = r;
            if estado
                .form
                .as_ref()
                .is_some_and(|f| f.pessoa == Some(pessoa))
            {
                estado.form = None;
            }
            estado.carregar(motor, sessao);
            notificar(ctx, Notificacao::sucesso("Cliente excluído"));
        }
        Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
    }
}
