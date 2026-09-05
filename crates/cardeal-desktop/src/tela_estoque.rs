//! A tela de Estoque — cadastro de produto + lista com saldo agregado.
//! `docs/modulos/estoque.md` §10.

use cardeal_cliente::{MotorLocal, SessaoLocal};
use cardeal_kernel::{Id, Preco, Quantidade};
use cardeal_ui::atoms::{Botao, Rotulo};
use cardeal_ui::molecules::{Campo, EstadoVazio, SeletorOpcao};
use cardeal_ui::organisms::{ColunaGrade, Grade, LayoutTela};
use cardeal_ui::tokens::{Espaco, TemaUi};
use cardeal_modkit::Icone;
use eframe::egui;
use mod_estoque::{
    CriarGrupoProduto, CriarLocal, CriarProduto, CriarUnidade, GrupoProdutoCriado, GruposProduto,
    ItemGrupoProduto, ItemLocal, ItemProdutoComSaldo, ItemUnidade, Locais, LocalCriado,
    ProdutoCriado, ProdutosComSaldo, RegistrarEntrada, TipoLocal, UnidadeCriada, Unidades,
};

/// O estado local da tela.
#[derive(Default)]
pub struct EstadoTelaEstoque {
    produtos: Vec<ItemProdutoComSaldo>,
    grupos: Vec<ItemGrupoProduto>,
    unidades: Vec<ItemUnidade>,
    locais: Vec<ItemLocal>,
    erro: Option<String>,

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
            Ok(produtos) => self.produtos = produtos,
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
}

/// Desenha a tela inteira.
pub fn mostrar(
    ui: &mut egui::Ui,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaEstoque,
) {
    LayoutTela::nova("Estoque").mostrar_com_detalhe(
        ui,
        estado,
        |ui, estado| {
            if ui.add(Botao::secundario("Recarregar").atalho("F5")).clicked() {
                estado.carregar(motor, sessao);
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
            mostrar_cadastro(ui, motor, sessao, estado);
        },
        |ui, estado| mostrar_lista(ui, estado),
    );
}

fn mostrar_cadastro(
    ui: &mut egui::Ui,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaEstoque,
) {
    ui.add(Rotulo::titulo_secao("Cadastrar produto"));
    ui.add_space(Espaco::E8);

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
                Ok(criado) => {
                    let criado: GrupoProdutoCriado = criado;
                    estado.grupo_selecionado = Some(criado.grupo_produto);
                    estado.novo_grupo_codigo.clear();
                    estado.novo_grupo_nome.clear();
                    estado.carregar(motor, sessao);
                }
                Err(e) => estado.erro = Some(e.mensagem),
            }
        }
    } else {
        let opcoes: Vec<(Id, String)> =
            estado.grupos.iter().map(|g| (g.id, g.nome.clone())).collect();
        SeletorOpcao::novo("Grupo", &mut estado.grupo_selecionado)
            .opcoes(opcoes)
            .mostrar(ui);
    }
    ui.add_space(Espaco::E12);

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
                Ok(criada) => {
                    let criada: UnidadeCriada = criada;
                    estado.unidade_selecionada = Some(criada.unidade);
                    estado.nova_unidade_sigla.clear();
                    estado.nova_unidade_nome.clear();
                    estado.carregar(motor, sessao);
                }
                Err(e) => estado.erro = Some(e.mensagem),
            }
        }
    } else {
        let opcoes: Vec<(Id, String)> = estado
            .unidades
            .iter()
            .map(|u| (u.id, u.sigla.clone()))
            .collect();
        SeletorOpcao::novo("Unidade", &mut estado.unidade_selecionada)
            .opcoes(opcoes)
            .mostrar(ui);
    }
    ui.add_space(Espaco::E12);

    ui.add(Campo::novo("Nome do produto", &mut estado.produto_nome));
    ui.add_space(Espaco::E8);
    ui.add(Campo::novo("NCM", &mut estado.produto_ncm).marcador("8 dígitos"));

    ui.add_space(Espaco::E24);
    ui.separator();
    ui.add_space(Espaco::E16);
    ui.add(Rotulo::titulo_secao("Local de estoque"));
    ui.add_space(Espaco::E8);
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
                Ok(criado) => {
                    let criado: LocalCriado = criado;
                    estado.local_selecionado = Some(criado.local);
                    estado.novo_local_nome.clear();
                    estado.carregar(motor, sessao);
                }
                Err(e) => estado.erro = Some(e.mensagem),
            }
        }
    } else {
        let opcoes: Vec<(Id, String)> =
            estado.locais.iter().map(|l| (l.id, l.nome.clone())).collect();
        SeletorOpcao::novo("Local", &mut estado.local_selecionado)
            .opcoes(opcoes)
            .mostrar(ui);
    }

    ui.add_space(Espaco::E12);
    ui.add(Rotulo::campo("Estoque inicial (opcional)"));
    ui.add(Campo::novo("Quantidade", &mut estado.estoque_inicial_qtd));
    ui.add_space(Espaco::E8);
    ui.add(Campo::novo("Custo unitário", &mut estado.estoque_inicial_custo).marcador("90,00"));

    ui.add_space(Espaco::E16);
    let pronto = estado.grupo_selecionado.is_some() && estado.unidade_selecionada.is_some();
    if ui
        .add(Botao::primario("Cadastrar produto").habilitado(pronto))
        .clicked()
        && pronto
    {
        cadastrar_produto(motor, sessao, estado);
    }
}

fn cadastrar_produto(motor: &MotorLocal, sessao: &SessaoLocal, estado: &mut EstadoTelaEstoque) {
    let resultado = motor.executar(
        sessao,
        "estoque.criar_produto.v1",
        &CriarProduto {
            grupo_produto: estado.grupo_selecionado.unwrap_or(Id::NULO),
            nome: estado.produto_nome.clone(),
            ncm: estado.produto_ncm.clone(),
            unidade_padrao: estado.unidade_selecionada.unwrap_or(Id::NULO),
        },
    );
    let criado: ProdutoCriado = match resultado {
        Ok(c) => c,
        Err(e) => {
            estado.erro = Some(e.mensagem);
            return;
        }
    };
    estado.produto_nome.clear();
    estado.produto_ncm.clear();

    if let (Some(local), Ok(quantidade), Ok(custo_unitario)) = (
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
                quantidade,
                custo_unitario,
            },
        ) {
            estado.erro = Some(e.mensagem);
        }
        estado.estoque_inicial_qtd.clear();
        estado.estoque_inicial_custo.clear();
    }
    estado.carregar(motor, sessao);
}

fn mostrar_lista(ui: &mut egui::Ui, estado: &EstadoTelaEstoque) {
    ui.add(Rotulo::titulo_secao("Produtos"));
    ui.add_space(Espaco::E8);
    if estado.produtos.is_empty() {
        EstadoVazio::novo(Icone::Estoque, "Nenhum produto cadastrado ainda.").mostrar(ui);
        return;
    }

    let colunas = vec![
        ColunaGrade::nova("Produto"),
        ColunaGrade::nova("NCM").largura(100.0),
        ColunaGrade::nova("Disponível").largura(110.0),
        ColunaGrade::nova("Reservado").largura(110.0),
        ColunaGrade::nova("Custo médio").largura(120.0),
    ];
    Grade::nova(colunas).mostrar(ui, estado.produtos.len(), |i, row| {
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
    });
}
