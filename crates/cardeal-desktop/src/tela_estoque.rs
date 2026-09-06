//! Tela de Estoque — lista de produtos + dialog (criar / ver). `docs/modulos/estoque.md`.
//!
//! Padrão de UI do projeto: a tela abre na lista inteira; "+ Novo produto" e o clique numa
//! linha abrem o mesmo `Dialogo`.

use cardeal_cliente::{MotorLocal, SessaoLocal};
use cardeal_kernel::{Id, Preco, Quantidade};
use cardeal_modkit::Icone;
use cardeal_ui::atoms::{Botao, Rotulo};
use cardeal_ui::molecules::{Campo, EstadoVazio, SeletorOpcao};
use cardeal_ui::organisms::{ColunaGrade, Dialogo, Grade, LayoutTela};
use cardeal_ui::tokens::{Espaco, TemaUi};
use eframe::egui;
use mod_estoque::{
    CriarGrupoProduto, CriarLocal, CriarProduto, CriarUnidade, GrupoProdutoCriado, GruposProduto,
    ItemGrupoProduto, ItemLocal, ItemProdutoComSaldo, ItemUnidade, Locais, LocalCriado,
    ProdutoCriado, ProdutosComSaldo, RegistrarEntrada, TipoLocal, UnidadeCriada, Unidades,
};

#[derive(Default, PartialEq)]
enum Dlg {
    #[default]
    Fechado,
    Novo,
    Ver(usize),
}

/// O estado local da tela.
#[derive(Default)]
pub struct EstadoTelaEstoque {
    produtos: Vec<ItemProdutoComSaldo>,
    grupos: Vec<ItemGrupoProduto>,
    unidades: Vec<ItemUnidade>,
    locais: Vec<ItemLocal>,
    erro: Option<String>,
    dlg: Dlg,

    novo_grupo_codigo: String,
    novo_grupo_nome: String,
    nova_unidade_sigla: String,
    nova_unidade_nome: String,
    novo_local_nome: String,

    grupo_selecionado: Option<Id>,
    unidade_selecionada: Option<Id>,
    produto_nome: String,
    produto_ncm: String,
    local_selecionado: Option<Id>,
    estoque_inicial_qtd: String,
    estoque_inicial_custo: String,
}

impl EstadoTelaEstoque {
    /// Recarrega produtos, grupos, unidades e locais.
    pub fn carregar(&mut self, motor: &MotorLocal, sessao: &SessaoLocal) {
        match motor.consultar(sessao, "estoque.produtos_com_saldo.v1", &ProdutosComSaldo) {
            Ok(p) => self.produtos = p,
            Err(e) => self.erro = Some(e.mensagem),
        }
        if let Ok(g) = motor.consultar(sessao, "estoque.grupos_produto.v1", &GruposProduto) {
            self.grupos = g;
        }
        if let Ok(u) = motor.consultar(sessao, "estoque.unidades.v1", &Unidades) {
            self.unidades = u;
        }
        if let Ok(l) = motor.consultar(sessao, "estoque.locais.v1", &Locais) {
            self.locais = l;
        }
    }

    fn limpar_novo(&mut self) {
        self.produto_nome.clear();
        self.produto_ncm.clear();
        self.estoque_inicial_qtd.clear();
        self.estoque_inicial_custo.clear();
    }
}

/// Desenha a tela inteira.
pub fn mostrar(
    ui: &mut egui::Ui,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaEstoque,
) {
    LayoutTela::nova("Estoque").mostrar(
        ui,
        estado,
        |ui, estado| {
            if ui.add(Botao::secundario("Recarregar").atalho("F5")).clicked() {
                estado.carregar(motor, sessao);
            }
            if ui.add(Botao::primario("+ Novo produto").atalho("Ctrl+N")).clicked() {
                estado.limpar_novo();
                estado.dlg = Dlg::Novo;
            }
        },
        |ui, estado| {
            if let Some(erro) = &estado.erro {
                ui.add(
                    Rotulo::interface(erro.clone())
                        .quebravel()
                        .cor(ui.cores().negativo),
                );
                ui.add_space(Espaco::E12);
            }
            lista(ui, estado);
        },
    );

    match estado.dlg {
        Dlg::Fechado => {}
        Dlg::Novo => dialogo_novo(ui.ctx(), motor, sessao, estado),
        Dlg::Ver(i) => dialogo_ver(ui.ctx(), estado, i),
    }
}

fn lista(ui: &mut egui::Ui, estado: &mut EstadoTelaEstoque) {
    if estado.produtos.is_empty() {
        if EstadoVazio::novo(Icone::Estoque, "Nenhum produto cadastrado ainda.")
            .acao("Cadastrar o primeiro")
            .mostrar(ui)
        {
            estado.limpar_novo();
            estado.dlg = Dlg::Novo;
        }
        return;
    }

    let colunas = vec![
        ColunaGrade::nova("Produto"),
        ColunaGrade::nova("NCM").largura(110.0),
        ColunaGrade::nova("Disponível").largura(110.0),
        ColunaGrade::nova("Reservado").largura(110.0),
        ColunaGrade::nova("Custo médio").largura(120.0),
    ];
    let clicada = Grade::nova(colunas).selecionavel(None).mostrar(
        ui,
        estado.produtos.len(),
        |i, row| {
            let p = &estado.produtos[i];
            row.col(|ui| {
                ui.add(Rotulo::interface(p.nome.clone()));
            });
            row.col(|ui| {
                ui.add(Rotulo::campo(p.ncm.clone()));
            });
            row.col(|ui| {
                ui.add(Rotulo::interface(p.disponivel.to_string()));
            });
            row.col(|ui| {
                ui.add(Rotulo::interface(p.reservado.to_string()));
            });
            row.col(|ui| {
                ui.add(Rotulo::interface(p.custo_medio.to_string()));
            });
        },
    );
    if let Some(i) = clicada {
        estado.dlg = Dlg::Ver(i);
    }
}

fn dialogo_ver(ctx: &egui::Context, estado: &mut EstadoTelaEstoque, i: usize) {
    let Some(p) = estado.produtos.get(i).cloned() else {
        estado.dlg = Dlg::Fechado;
        return;
    };
    let fechar = Dialogo::nova(p.nome.clone()).largura(620.0).mostrar(
        ctx,
        estado,
        |ui, _estado| {
            ui.columns(2, |c| {
                campo_ver(&mut c[0], "NCM", &p.ncm);
                campo_ver(&mut c[1], "Custo médio", &p.custo_medio.to_string());
            });
            ui.columns(2, |c| {
                campo_ver(&mut c[0], "Disponível", &p.disponivel.to_string());
                campo_ver(&mut c[1], "Reservado", &p.reservado.to_string());
            });
        },
        |ui, estado| {
            if ui.add(Botao::secundario("Fechar")).clicked() {
                estado.dlg = Dlg::Fechado;
            }
        },
    );
    if fechar {
        estado.dlg = Dlg::Fechado;
    }
}

fn campo_ver(ui: &mut egui::Ui, chave: &str, valor: &str) {
    ui.add(Rotulo::campo(chave));
    ui.add(Rotulo::interface(if valor.trim().is_empty() { "—" } else { valor }));
    ui.add_space(Espaco::E12);
}

fn dialogo_novo(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaEstoque,
) {
    let fechar = Dialogo::nova("Novo produto").largura(680.0).mostrar(
        ctx,
        estado,
        |ui, estado| corpo_novo(ui, motor, sessao, estado),
        |ui, estado| {
            let pronto =
                estado.grupo_selecionado.is_some() && estado.unidade_selecionada.is_some();
            if ui
                .add(Botao::primario("Cadastrar produto").habilitado(pronto))
                .clicked()
                && pronto
            {
                cadastrar(motor, sessao, estado);
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

fn corpo_novo(
    ui: &mut egui::Ui,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaEstoque,
) {
    let grupo_pronto = !estado.grupos.is_empty();
    let unidade_pronta = !estado.unidades.is_empty();

    if grupo_pronto && unidade_pronta {
        ui.columns(2, |c| {
            bloco_grupo(&mut c[0], motor, sessao, estado);
            bloco_unidade(&mut c[1], motor, sessao, estado);
        });
    } else {
        bloco_grupo(ui, motor, sessao, estado);
        ui.add_space(Espaco::E12);
        bloco_unidade(ui, motor, sessao, estado);
    }
    ui.add_space(Espaco::E12);

    ui.columns(2, |c| {
        c[0].add(Campo::novo("Nome do produto", &mut estado.produto_nome));
        c[1].add(Campo::novo("NCM", &mut estado.produto_ncm).marcador("8 dígitos"));
    });

    ui.add_space(Espaco::E16);
    ui.separator();
    ui.add_space(Espaco::E12);
    ui.add(Rotulo::campo("Estoque inicial (opcional)"));
    ui.add_space(Espaco::E8);
    bloco_local(ui, motor, sessao, estado);
    ui.add_space(Espaco::E8);
    ui.columns(2, |c| {
        c[0].add(Campo::novo("Quantidade", &mut estado.estoque_inicial_qtd));
        c[1].add(Campo::novo("Custo unitário", &mut estado.estoque_inicial_custo).marcador("90,00"));
    });
}

fn bloco_grupo(
    ui: &mut egui::Ui,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaEstoque,
) {
    if estado.grupos.is_empty() {
        ui.add(Rotulo::campo("Nenhum grupo ainda — crie um:"));
        ui.add(Campo::novo("Código", &mut estado.novo_grupo_codigo).marcador("PECAS"));
        ui.add_space(Espaco::E8);
        ui.add(Campo::novo("Nome do grupo", &mut estado.novo_grupo_nome));
        ui.add_space(Espaco::E8);
        if ui.add(Botao::secundario("Criar grupo")).clicked() {
            match motor.executar(
                sessao,
                "estoque.criar_grupo_produto.v1",
                &CriarGrupoProduto {
                    codigo: estado.novo_grupo_codigo.clone(),
                    nome: estado.novo_grupo_nome.clone(),
                    pai: None,
                },
            ) {
                Ok(c) => {
                    let c: GrupoProdutoCriado = c;
                    estado.grupo_selecionado = Some(c.grupo_produto);
                    estado.novo_grupo_codigo.clear();
                    estado.novo_grupo_nome.clear();
                    estado.carregar(motor, sessao);
                    estado.dlg = Dlg::Novo;
                }
                Err(e) => estado.erro = Some(e.mensagem),
            }
        }
    } else {
        let ops: Vec<(Id, String)> =
            estado.grupos.iter().map(|g| (g.id, g.nome.clone())).collect();
        SeletorOpcao::novo("Grupo", &mut estado.grupo_selecionado)
            .opcoes(ops)
            .mostrar(ui);
    }
}

fn bloco_unidade(
    ui: &mut egui::Ui,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaEstoque,
) {
    if estado.unidades.is_empty() {
        ui.add(Rotulo::campo("Nenhuma unidade ainda — crie uma:"));
        ui.add(Campo::novo("Sigla", &mut estado.nova_unidade_sigla).marcador("UN"));
        ui.add_space(Espaco::E8);
        ui.add(Campo::novo("Nome da unidade", &mut estado.nova_unidade_nome));
        ui.add_space(Espaco::E8);
        if ui.add(Botao::secundario("Criar unidade")).clicked() {
            match motor.executar(
                sessao,
                "estoque.criar_unidade.v1",
                &CriarUnidade {
                    sigla: estado.nova_unidade_sigla.clone(),
                    nome: estado.nova_unidade_nome.clone(),
                    fracionavel: false,
                },
            ) {
                Ok(c) => {
                    let c: UnidadeCriada = c;
                    estado.unidade_selecionada = Some(c.unidade);
                    estado.nova_unidade_sigla.clear();
                    estado.nova_unidade_nome.clear();
                    estado.carregar(motor, sessao);
                    estado.dlg = Dlg::Novo;
                }
                Err(e) => estado.erro = Some(e.mensagem),
            }
        }
    } else {
        let ops: Vec<(Id, String)> = estado
            .unidades
            .iter()
            .map(|u| (u.id, u.sigla.clone()))
            .collect();
        SeletorOpcao::novo("Unidade", &mut estado.unidade_selecionada)
            .opcoes(ops)
            .mostrar(ui);
    }
}

fn bloco_local(
    ui: &mut egui::Ui,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaEstoque,
) {
    if estado.locais.is_empty() {
        ui.add(Campo::novo("Nome do local", &mut estado.novo_local_nome).marcador("Depósito"));
        ui.add_space(Espaco::E8);
        if ui.add(Botao::secundario("Criar local")).clicked() {
            match motor.executar(
                sessao,
                "estoque.criar_local.v1",
                &CriarLocal {
                    nome: estado.novo_local_nome.clone(),
                    tipo: TipoLocal::Deposito,
                },
            ) {
                Ok(c) => {
                    let c: LocalCriado = c;
                    estado.local_selecionado = Some(c.local);
                    estado.novo_local_nome.clear();
                    estado.carregar(motor, sessao);
                    estado.dlg = Dlg::Novo;
                }
                Err(e) => estado.erro = Some(e.mensagem),
            }
        }
    } else {
        let ops: Vec<(Id, String)> =
            estado.locais.iter().map(|l| (l.id, l.nome.clone())).collect();
        SeletorOpcao::novo("Local", &mut estado.local_selecionado)
            .opcoes(ops)
            .mostrar(ui);
    }
}

fn cadastrar(motor: &MotorLocal, sessao: &SessaoLocal, estado: &mut EstadoTelaEstoque) {
    let r = motor.executar(
        sessao,
        "estoque.criar_produto.v1",
        &CriarProduto {
            grupo_produto: estado.grupo_selecionado.unwrap_or(Id::NULO),
            nome: estado.produto_nome.clone(),
            ncm: estado.produto_ncm.clone(),
            unidade_padrao: estado.unidade_selecionada.unwrap_or(Id::NULO),
        },
    );
    let criado: ProdutoCriado = match r {
        Ok(c) => c,
        Err(e) => {
            estado.erro = Some(e.mensagem);
            return;
        }
    };

    if let (Some(local), Ok(qtd), Ok(custo)) = (
        estado.local_selecionado,
        estado.estoque_inicial_qtd.parse::<Quantidade>(),
        estado.estoque_inicial_custo.parse::<Preco>(),
    ) {
        if let Err(e) = motor.executar(
            sessao,
            "estoque.registrar_entrada.v1",
            &RegistrarEntrada {
                produto: criado.produto,
                local,
                quantidade: qtd,
                custo_unitario: custo,
            },
        ) {
            estado.erro = Some(e.mensagem);
        }
    }

    estado.limpar_novo();
    estado.dlg = Dlg::Fechado;
    estado.carregar(motor, sessao);
}
