//! Tela de Clientes — lista + dialog (criar / ver / editar). `docs/modulos/clientes.md`.
//!
//! É o caso de cadastro puro: o dialog tem os três modos da regra de UI do projeto —
//! **Criar** (campos vazios), **Ver** (campos como texto + botão "Editar"), **Editar**
//! (campos de novo editáveis, Salvar / Cancelar).

use cardeal_cliente::{MotorLocal, SessaoLocal};
use cardeal_kernel::Id;
use cardeal_modkit::Icone;
use cardeal_ui::atoms::{Botao, Rotulo};
use cardeal_ui::molecules::{Campo, EstadoVazio, Mascara, SeletorOpcao};
use cardeal_ui::organisms::{ColunaGrade, Dialogo, Grade, LayoutTela};
use cardeal_ui::tokens::{Espaco, TemaUi};
use eframe::egui;
use mod_clientes::{
    CriarPessoa, DetalhePessoa, EditarPessoa, ItemPessoa, Papel, PessoaCadastrada, PessoaDetalhada,
    PessoaEditada, PessoasPorPapel, TipoDocumento, TipoPessoa,
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
    // Só leitura (modo Ver):
    papeis: String,
    limite: String,
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
            papeis: String::new(),
            limite: String::new(),
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
            papeis,
            limite: d
                .limite_credito
                .as_ref()
                .map(|l| l.limite.formatar_com_simbolo())
                .unwrap_or_default(),
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

/// Estado local da tela.
#[derive(Default)]
pub struct EstadoTelaClientes {
    pessoas: Vec<ItemPessoa>,
    busca: String,
    erro: Option<String>,
    form: Option<Form>,
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
            }
            Err(e) => self.erro = Some(e.mensagem),
        }
    }

    fn abrir_detalhe(&mut self, motor: &MotorLocal, sessao: &SessaoLocal, id: Id) {
        match motor.consultar(sessao, "clientes.detalhe_pessoa.v1", &DetalhePessoa { pessoa: id }) {
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
            if ui.add(Botao::primario("+ Novo cliente").atalho("Ctrl+N")).clicked() {
                estado.form = Some(Form::novo());
            }
        },
        |ui, estado| {
            ui.horizontal(|ui| {
                ui.set_max_width(360.0);
                if ui
                    .add(cardeal_ui::molecules::Campo::novo("", &mut estado.busca).marcador("Buscar por nome ou documento"))
                    .changed()
                {
                    estado.carregar(motor, sessao);
                }
            });
            ui.add_space(Espaco::E12);

            if let Some(erro) = &estado.erro {
                ui.add(Rotulo::interface(erro.clone()).quebravel().cor(ui.cores().negativo));
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
        if EstadoVazio::novo(Icone::Pessoas, msg)
            .acao("Cadastrar cliente")
            .mostrar(ui)
        {
            estado.form = Some(Form::novo());
        }
        return;
    }

    let colunas = vec![
        ColunaGrade::nova("Nome"),
        ColunaGrade::nova("Documento").largura(200.0),
    ];
    let clicada = Grade::nova(colunas).selecionavel(None).mostrar(
        ui,
        estado.pessoas.len(),
        |i, row| {
            let p = &estado.pessoas[i];
            row.col(|ui| {
                ui.add(Rotulo::interface(p.nome.clone()));
            });
            row.col(|ui| {
                ui.add(Rotulo::campo(p.documento.clone().unwrap_or_else(|| "—".to_owned())));
            });
        },
    );
    if let Some(i) = clicada {
        let id = estado.pessoas[i].pessoa;
        estado.abrir_detalhe(motor, sessao, id);
    }
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

    let fechar = Dialogo::nova(titulo).largura(640.0).mostrar(
        ctx,
        estado,
        |ui, estado| {
            let Some(f) = estado.form.as_mut() else { return };
            let leitura = f.modo == Modo::Ver;
            let pj = matches!(f.tipo, Some(TipoPessoa::Juridica));

            if f.modo == Modo::Criar {
                SeletorOpcao::novo("Tipo", &mut f.tipo)
                    .opcao(TipoPessoa::Fisica, "Pessoa física")
                    .opcao(TipoPessoa::Juridica, "Pessoa jurídica")
                    .mostrar(ui);
                ui.add_space(Espaco::E12);
            }

            ui.columns(2, |c| {
                c[0].add(
                    Campo::novo(if pj { "Razão social" } else { "Nome" }, &mut f.nome)
                        .somente_leitura(leitura),
                );
                c[1].add(
                    Campo::novo(if pj { "CNPJ" } else { "CPF" }, &mut f.documento)
                        .mascara(Mascara::Documento)
                        .somente_leitura(leitura || f.modo == Modo::Editar),
                );
            });
            ui.add_space(Espaco::E12);

            if pj {
                ui.add(
                    Campo::novo("Nome fantasia", &mut f.nome_fantasia).somente_leitura(leitura),
                );
                ui.add_space(Espaco::E12);
            }

            ui.add(Campo::novo("Observação", &mut f.observacao).somente_leitura(leitura));

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
                    if ui.add(Botao::primario("Cadastrar")).clicked() {
                        criar(motor, sessao, estado);
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
                    if ui.add(Botao::primario("Salvar")).clicked() {
                        salvar(motor, sessao, estado);
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
    ui.add(Rotulo::interface(if valor.trim().is_empty() { "—" } else { valor }));
}

fn criar(motor: &MotorLocal, sessao: &SessaoLocal, estado: &mut EstadoTelaClientes) {
    let Some(f) = estado.form.as_ref() else { return };
    let pj = matches!(f.tipo, Some(TipoPessoa::Juridica));
    let cmd = CriarPessoa {
        tipo: f.tipo.unwrap_or(TipoPessoa::Fisica),
        nome: f.nome.clone(),
        nome_fantasia: (pj && !f.nome_fantasia.trim().is_empty())
            .then(|| f.nome_fantasia.clone()),
        papel_inicial: Papel::Cliente,
        documento_tipo: if pj { TipoDocumento::Cnpj } else { TipoDocumento::Cpf },
        documento_numero: f.documento.clone(),
    };
    match motor.executar(sessao, "clientes.criar_pessoa.v1", &cmd) {
        Ok(r) => {
            let _: PessoaCadastrada = r;
            estado.form = None;
            estado.carregar(motor, sessao);
        }
        Err(e) => estado.erro = Some(e.mensagem),
    }
}

fn salvar(motor: &MotorLocal, sessao: &SessaoLocal, estado: &mut EstadoTelaClientes) {
    let Some(f) = estado.form.as_ref() else { return };
    let Some(id) = f.pessoa else { return };
    let cmd = EditarPessoa {
        pessoa: id,
        nome: Some(f.nome.clone()),
        nome_fantasia: Some(f.nome_fantasia.clone()).filter(|s| !s.trim().is_empty()),
        observacao: Some(f.observacao.clone()).filter(|s| !s.trim().is_empty()),
    };
    match motor.executar(sessao, "clientes.editar_pessoa.v1", &cmd) {
        Ok(r) => {
            let _: PessoaEditada = r;
            estado.abrir_detalhe(motor, sessao, id);
            estado.carregar(motor, sessao);
        }
        Err(e) => estado.erro = Some(e.mensagem),
    }
}
