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
use cardeal_ui::molecules::{Campo, CartaoKpi, EstadoVazio, Mascara, SeletorOpcao};
use cardeal_ui::organisms::{
    notificar, ColunaGrade, Dialogo, Direcao, FaixaKpi, Grade, LayoutTela, Notificacao,
};
use cardeal_ui::tokens::{Espaco, TemaUi};
use eframe::egui;
use mod_clientes::{
    CriarPessoa, DetalhePessoa, EditarPessoa, ItemPessoa, Papel, PessoaCadastrada, PessoaDetalhada,
    PessoaEditada, PessoasPorPapel, TipoContato, TipoDocumento, TipoPessoa,
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
    telefone: String,
    email: String,
    endereco: String,
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
            endereco: String::new(),
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
        let endereco = d.enderecos.first().map_or_else(String::new, |e| {
            format!(
                "{}, {} - {}, {}/{}",
                e.logradouro, e.numero, e.bairro, e.cidade, e.uf
            )
        });
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
            telefone,
            email,
            endereco,
        }
    }
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
        ColunaGrade::nova("Documento").largura(220.0),
    ];
    let resposta = Grade::nova(colunas)
        .selecionavel(None)
        .ordenacao(estado.ordenacao)
        .mostrar(ui, estado.pessoas.len(), |i, row| {
            let p = &estado.pessoas[i];
            row.col(|ui| {
                ui.add(Rotulo::interface(p.nome.clone()));
            });
            row.col(|ui| match &p.documento {
                Some(doc) => {
                    ui.add(Rotulo::campo(doc.clone()));
                }
                // TODO(backend): `ItemPessoa` (consulta `clientes.pessoas_por_papel.v1`) não
                // traz telefone — só documento. Um `telefone_principal: Option<String>` ali
                // deixaria a lista mostrar contato sem abrir o dialog "Ver" a cada linha, o
                // que ajudaria bastante no balcão (ligar/whatsapp rápido).
                None => {
                    ui.add(Etiqueta::atencao("sem documento"));
                }
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
    if let Some(i) = resposta.linha_clicada {
        let id = estado.pessoas[i].pessoa;
        estado.abrir_detalhe(motor, sessao, id);
    }
}

/// Ordena `pessoas` pela coluna clicada no cabeçalho da [`Grade`] (Nome, Documento).
fn ordenar_pessoas(pessoas: &mut [ItemPessoa], coluna: usize, direcao: Direcao) {
    pessoas.sort_by(|a, b| {
        let ordem = match coluna {
            0 => a.nome.cmp(&b.nome),
            1 => a.documento.cmp(&b.documento),
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

            if f.modo == Modo::Ver {
                ui.add_space(Espaco::E16);
                ui.separator();
                ui.add_space(Espaco::E12);
                ui.columns(2, |c| {
                    kv(&mut c[0], "Telefone", &f.telefone);
                    kv(&mut c[1], "E-mail", &f.email);
                });
                ui.add_space(Espaco::E12);
                kv(ui, "Endereço", &f.endereco);
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
        endereco: None,
        contato: None,
    };
    match motor.executar(sessao, "clientes.criar_pessoa.v1", &cmd) {
        Ok(r) => {
            let _: PessoaCadastrada = r;
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
            estado.abrir_detalhe(motor, sessao, id);
            estado.carregar(motor, sessao);
            notificar(ctx, Notificacao::sucesso("Cliente atualizado"));
        }
        Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
    }
}
