//! A aba "Estrutura": a árvore mercadológica de até 4 níveis (Grupo › Subgrupo › Categoria ›
//! Subcategoria) com a margem desejada, a comissão de vendedor e o perfil fiscal de cada nó.
//! Margem, comissão e perfil são herdados pelos filhos e pelos produtos até que alguém defina
//! um valor mais próximo.

use std::collections::HashSet;

use super::*;
use cardeal_kernel::Percentual;
use cardeal_ui::organisms::{ArvoreHierarquia, EstadoSeletorArvore, NoArvore, Painel};
use mod_estoque::hierarquia::{nome_do_nivel, Arvore, NoGrupo, NIVEL_MAXIMO};
use mod_estoque::{
    ArvoreGruposProduto, DefinirComissaoDoGrupo, DefinirMargemDoGrupo, DefinirPerfilFiscalDoGrupo,
    MoverGrupoProduto, NoArvoreGrupo, RenomearGrupoProduto,
};

/// O estado da aba.
#[derive(Default)]
pub(super) struct EstadoEstrutura {
    pub(super) nos: Vec<NoArvoreGrupo>,
    recolhidos: HashSet<Id>,
    selecionado: Option<Id>,
    codigo: String,
    nome: String,
    margem: String,
    pub(super) comissao: String,
    /// 0 = herda, 1 = sim, 2 = não.
    st: Option<u8>,
    monofasico: Option<u8>,
    /// Destino do "Mover para": `Some(Id::NULO)` = raiz.
    mover_para: Option<Id>,
    novo_codigo: String,
    novo_nome: String,
    fisc: super::fiscal::FormFiscal,
    /// A busca/recolhidos de cada `SeletorEmArvore` da tela de estoque.
    pub(super) imp: super::estrutura_importacao::EstadoImportacao,
    pub(super) sel_cadastro: EstadoSeletorArvore,
    pub(super) sel_filtro_lote: EstadoSeletorArvore,
    pub(super) sel_destino_lote: EstadoSeletorArvore,
    pub(super) sel_filtro_precos: EstadoSeletorArvore,
}

fn tri(v: Option<bool>) -> u8 {
    match v {
        None => 0,
        Some(true) => 1,
        Some(false) => 2,
    }
}

const fn de_tri(v: u8) -> Option<bool> {
    match v {
        1 => Some(true),
        2 => Some(false),
        _ => None,
    }
}

impl EstadoEstrutura {
    pub(super) fn carregar(&mut self, motor: &Motor, sessao: &Sessao) {
        if let Ok(n) = motor.consultar(
            sessao,
            "estoque.arvore_grupos_produto.v1",
            &ArvoreGruposProduto,
        ) {
            self.nos = n;
        }
        if self.selecionado.is_some_and(|s| self.no(s).is_none()) {
            self.selecionado = None;
        }
        self.preencher_formulario();
    }

    fn no(&self, id: Id) -> Option<&NoArvoreGrupo> {
        self.nos.iter().find(|n| n.id == id)
    }

    fn arvore(&self) -> Arvore {
        Arvore::de_nos(
            &self
                .nos
                .iter()
                .map(|n| NoGrupo {
                    id: n.id,
                    pai: n.pai,
                })
                .collect::<Vec<_>>(),
        )
    }

    pub(super) fn selecionar(&mut self, id: Id) {
        self.selecionado = Some(id);
        self.preencher_formulario();
    }

    fn preencher_formulario(&mut self) {
        let Some(n) = self.selecionado.and_then(|s| self.no(s)).cloned() else {
            return;
        };
        self.codigo = n.codigo;
        self.nome = n.nome;
        self.margem = n.margem.map(|m| m.formatar(2)).unwrap_or_default();
        self.comissao = n.comissao.map(|c| c.formatar(2)).unwrap_or_default();
        self.st = Some(tri(n.icms_st));
        self.monofasico = Some(tri(n.monofasico));
        self.fisc = super::fiscal::FormFiscal::de(&n.fiscal);
        self.mover_para = None;
    }

    /// A margem herdada de quem está acima do nó, e de qual grupo vem.
    pub(super) fn margem_herdada(&self, id: Id) -> Option<(Percentual, String)> {
        self.herdado(id, |n| n.margem)
    }

    /// A comissão herdada de quem está acima do nó, e de qual grupo vem.
    pub(super) fn comissao_herdada(&self, id: Id) -> Option<(Percentual, String)> {
        self.herdado(id, |n| n.comissao)
    }

    /// O primeiro valor definido acima do nó (sem contar ele), e o nome de quem definiu.
    fn herdado(
        &self,
        id: Id,
        valor: impl Fn(&NoArvoreGrupo) -> Option<Percentual>,
    ) -> Option<(Percentual, String)> {
        self.arvore()
            .ancestrais(id)
            .into_iter()
            .skip(1)
            .find_map(|g| {
                self.no(g)
                    .and_then(|n| valor(n).map(|m| (m, n.nome.clone())))
            })
    }

    /// Os grupos como nós para o `SeletorEmArvore` (pré-ordem, sem texto de apoio).
    pub(super) fn nos_arvore(&self) -> Vec<NoArvore> {
        let mut v: Vec<NoArvore> = self
            .nos
            .iter()
            .map(|n| NoArvore {
                id: n.id,
                nivel: n.nivel,
                rotulo: n.nome.clone(),
                apoio: None,
                tem_filhos: false,
            })
            .collect();
        for i in 0..v.len() {
            v[i].tem_filhos = v.get(i + 1).is_some_and(|p| p.nivel > v[i].nivel);
        }
        v
    }

    /// Todos os grupos com o caminho completo ("Bebidas › Refrigerantes"), para seletores.
    pub(super) fn nos_para_seletor(&self) -> Vec<(Id, String)> {
        let arvore = self.arvore();
        self.nos
            .iter()
            .map(|n| {
                let mut caminho: Vec<&str> = arvore
                    .ancestrais(n.id)
                    .iter()
                    .filter_map(|g| self.no(*g).map(|x| x.nome.as_str()))
                    .collect();
                caminho.reverse();
                (n.id, caminho.join(" › "))
            })
            .collect()
    }

    /// Para onde o grupo pode ir sem estourar 4 níveis nem virar ciclo.
    pub(super) fn destinos_validos(&self, id: Id) -> Vec<(Id, String)> {
        let arvore = self.arvore();
        let atual = self.no(id).and_then(|n| n.pai);
        let mut v = Vec::new();
        if atual.is_some() && arvore.validar_movimento(id, None).is_ok() {
            v.push((Id::NULO, "(sem grupo — primeiro nível)".to_owned()));
        }
        for n in &self.nos {
            if n.id != id && Some(n.id) != atual && arvore.validar_movimento(id, Some(n.id)).is_ok()
            {
                v.push((n.id, format!("{} › {}", nome_do_nivel(n.nivel), n.nome)));
            }
        }
        v
    }
}

pub(super) fn aba(
    ui: &mut egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaEstoque,
) {
    ui.add(Rotulo::apoio(
        "Organize o catálogo em até quatro níveis. A margem desejada e o perfil fiscal valem \
         para tudo que está abaixo, até alguém definir um valor mais próximo. Para usar só \
         Grupo e Subgrupo, simplesmente não crie os níveis de baixo.",
    ));
    ui.add_space(Espaco::E8);
    if ui.add(Botao::secundario("Importar planilha…")).clicked() {
        estado.est.imp.limpar();
        estado.dlg = Dlg::ImportarEstrutura;
    }
    ui.add_space(Espaco::E12);
    ui.columns(2, |c| {
        arvore(&mut c[0], estado);
        detalhe(&mut c[1], motor, sessao, estado);
    });
}

fn arvore(ui: &mut egui::Ui, estado: &mut EstadoTelaEstoque) {
    let e = &mut estado.est;
    let nos: Vec<NoArvore> = e
        .nos
        .iter()
        .map(|n| NoArvore {
            id: n.id,
            nivel: n.nivel,
            rotulo: n.nome.clone(),
            apoio: Some(format!(
                "{} · {} produto(s)",
                n.codigo, n.produtos_na_subarvore
            )),
            tem_filhos: e.nos.iter().any(|f| f.pai == Some(n.id)),
        })
        .collect();
    if nos.is_empty() {
        EstadoVazio::novo(
            Icone::Estoque,
            "Nenhum grupo ainda. Crie o primeiro ao lado — por exemplo \"Bebidas\" ou \"Peças\".",
        )
        .mostrar(ui);
        return;
    }
    let por_id: Vec<NoArvoreGrupo> = e.nos.clone();
    let resposta = ArvoreHierarquia::nova(&nos, &mut e.recolhidos)
        .selecionado(e.selecionado)
        .mostrar(ui, |ui, no| {
            let Some(g) = por_id.iter().find(|g| g.id == no.id) else {
                return;
            };
            ui.add(Etiqueta::neutra(nome_do_nivel(no.nivel)));
            if let Some(m) = g.margem {
                ui.add(Etiqueta::info(format!("margem {}%", m.formatar(1))));
            }
            if let Some(c) = g.comissao {
                ui.add(Etiqueta::info(format!("comissão {}%", c.formatar(1))));
            }
            if g.icms_st == Some(true) {
                ui.add(Etiqueta::atencao("ST"));
            }
            if g.monofasico == Some(true) {
                ui.add(Etiqueta::atencao("monofásico"));
            }
        });
    if let Some(id) = resposta.clicado {
        e.selecionar(id);
    }
}

fn detalhe(ui: &mut egui::Ui, motor: &Motor, sessao: &Sessao, estado: &mut EstadoTelaEstoque) {
    let selecionado = estado
        .est
        .selecionado
        .and_then(|s| estado.est.no(s).cloned());
    let Some(no) = selecionado else {
        Painel::novo().plano().mostrar(ui, |ui| {
            ui.add(Rotulo::titulo_secao("Novo grupo"));
            ui.add(Rotulo::apoio(
                "Escolha um grupo na árvore para editar, ou crie um de primeiro nível aqui.",
            ));
            ui.add_space(Espaco::E8);
            formulario_novo(ui, motor, sessao, estado, None);
        });
        return;
    };

    Painel::novo().plano().mostrar(ui, |ui| {
        ui.add(Rotulo::sobrelinha(nome_do_nivel(no.nivel)));
        ui.add(Rotulo::titulo_secao(no.nome.clone()));
        ui.add(Rotulo::apoio(format!(
            "{} produto(s) direto(s) · {} contando o que está abaixo",
            no.produtos_diretos, no.produtos_na_subarvore
        )));
        ui.add_space(Espaco::E12);

        bloco_nome(ui, motor, sessao, estado, &no);
        ui.add(Divisor::novo());
        bloco_margem(ui, motor, sessao, estado, &no);
        ui.add(Divisor::novo());
        bloco_comissao(ui, motor, sessao, estado, &no);
        ui.add(Divisor::novo());
        bloco_perfil(ui, motor, sessao, estado, &no);
        ui.add(Divisor::novo());
        bloco_fiscal(ui, motor, sessao, estado, &no);
        ui.add(Divisor::novo());
        bloco_mover(ui, motor, sessao, estado, &no);
        ui.add(Divisor::novo());
        if no.nivel < NIVEL_MAXIMO {
            ui.add(Rotulo::titulo_secao(format!(
                "Novo {} dentro de {}",
                nome_do_nivel(no.nivel + 1).to_lowercase(),
                no.nome
            )));
            ui.add_space(Espaco::E8);
            formulario_novo(ui, motor, sessao, estado, Some(no.id));
        } else {
            ui.add(Rotulo::apoio(
                "Este é o último nível: não dá para criar nada abaixo.",
            ));
        }
        ui.add_space(Espaco::E8);
        if ui.add(Botao::fantasma("Desmarcar")).clicked() {
            estado.est.selecionado = None;
        }
    });
}

fn aplicar(
    ctx: &egui::Context,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaEstoque,
    resultado: Result<(), cardeal_kernel::Erro>,
    sucesso: &str,
) {
    match resultado {
        Ok(()) => {
            estado.carregar(motor, sessao);
            notificar(ctx, Notificacao::sucesso(sucesso));
        }
        Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
    }
}

fn bloco_nome(
    ui: &mut egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaEstoque,
    no: &NoArvoreGrupo,
) {
    ui.columns(2, |c| {
        c[0].add(Campo::novo("Código", &mut estado.est.codigo));
        c[1].add(Campo::novo("Nome", &mut estado.est.nome));
    });
    ui.add_space(Espaco::E8);
    let mudou = estado.est.codigo.trim() != no.codigo || estado.est.nome.trim() != no.nome;
    if ui
        .add_enabled(mudou, Botao::secundario("Renomear"))
        .clicked()
    {
        let r = motor.executar(
            sessao,
            "estoque.renomear_grupo_produto.v1",
            &RenomearGrupoProduto {
                grupo: no.id,
                codigo: estado.est.codigo.clone(),
                nome: estado.est.nome.clone(),
            },
        );
        aplicar(ui.ctx(), motor, sessao, estado, r, "Grupo renomeado");
    }
    ui.add_space(Espaco::E12);
}

/// O que o usuário pediu num bloco de percentual herdado.
enum AcaoHerdado {
    Nenhuma,
    Definir(Percentual),
    Herdar,
}

/// Os textos de um bloco de percentual herdado pela árvore.
struct TextosHerdado {
    titulo: &'static str,
    campo: &'static str,
    marcador: &'static str,
    ninguem: &'static str,
    salvar: &'static str,
    erro_numero: &'static str,
}

/// Um percentual herdado pela árvore (margem, comissão): de onde vem o que vale, o campo para
/// definir neste grupo e o "Voltar a herdar".
fn bloco_herdado(
    ui: &mut egui::Ui,
    t: &TextosHerdado,
    texto: &mut String,
    proprio: Option<Percentual>,
    herdado: Option<(Percentual, String)>,
) -> AcaoHerdado {
    ui.add(Rotulo::titulo_secao(t.titulo));
    ui.add(Rotulo::apoio(match (proprio, &herdado) {
        (Some(_), _) => "Definida neste grupo.".to_owned(),
        (None, Some((m, de))) => format!("Herdada de \"{de}\": {}%.", m.formatar(2)),
        (None, None) => t.ninguem.to_owned(),
    }));
    ui.add_space(Espaco::E8);
    ui.add(Campo::novo(t.campo, texto).marcador(t.marcador));
    ui.add_space(Espaco::E8);
    let mut acao = AcaoHerdado::Nenhuma;
    ui.horizontal(|ui| {
        if ui.add(Botao::secundario(t.salvar)).clicked() {
            match Percentual::de_str(texto.trim()) {
                Ok(m) => acao = AcaoHerdado::Definir(m),
                Err(_) => notificar(ui.ctx(), Notificacao::erro(t.erro_numero)),
            }
        }
        if proprio.is_some() && ui.add(Botao::fantasma("Voltar a herdar")).clicked() {
            acao = AcaoHerdado::Herdar;
        }
    });
    ui.add_space(Espaco::E12);
    acao
}

fn bloco_margem(
    ui: &mut egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaEstoque,
    no: &NoArvoreGrupo,
) {
    let herdada = estado.est.margem_herdada(no.id);
    let textos = TextosHerdado {
        titulo: "Margem líquida desejada",
        campo: "Margem (%)",
        marcador: "ex.: 20",
        ninguem: "Ninguém definiu: vale a margem padrão da empresa (aba Preços).",
        salvar: "Salvar margem",
        erro_numero: "Digite a margem como número, por exemplo 20 ou 12,5",
    };
    let (margem, aviso) =
        match bloco_herdado(ui, &textos, &mut estado.est.margem, no.margem, herdada) {
            AcaoHerdado::Nenhuma => return,
            AcaoHerdado::Definir(m) => (Some(m), "Margem salva"),
            AcaoHerdado::Herdar => (None, "Margem volta a ser herdada"),
        };
    let r = motor.executar(
        sessao,
        "estoque.definir_margem_do_grupo.v1",
        &DefinirMargemDoGrupo {
            grupo: no.id,
            margem,
        },
    );
    aplicar(ui.ctx(), motor, sessao, estado, r, aviso);
}

fn bloco_comissao(
    ui: &mut egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaEstoque,
    no: &NoArvoreGrupo,
) {
    let herdada = estado.est.comissao_herdada(no.id);
    let textos = TextosHerdado {
        titulo: "Comissão de vendedor",
        campo: "Comissão (%)",
        marcador: "ex.: 3",
        ninguem: "Ninguém definiu: vale a comissão padrão da empresa (aba Preços). Ela entra \
                  no preço sugerido, nos painéis de margem e na rentabilidade.",
        salvar: "Salvar comissão",
        erro_numero: "Digite a comissão como número, por exemplo 3 ou 2,5",
    };
    let (comissao, aviso) =
        match bloco_herdado(ui, &textos, &mut estado.est.comissao, no.comissao, herdada) {
            AcaoHerdado::Nenhuma => return,
            AcaoHerdado::Definir(c) => (Some(c), "Comissão salva"),
            AcaoHerdado::Herdar => (None, "Comissão volta a ser herdada"),
        };
    let r = motor.executar(
        sessao,
        "estoque.definir_comissao_do_grupo.v1",
        &DefinirComissaoDoGrupo {
            grupo: no.id,
            comissao,
        },
    );
    aplicar(ui.ctx(), motor, sessao, estado, r, aviso);
}

fn bloco_perfil(
    ui: &mut egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaEstoque,
    no: &NoArvoreGrupo,
) {
    ui.add(Rotulo::titulo_secao("Perfil fiscal (Simples Nacional)"));
    ui.add(Rotulo::apoio(
        "ICMS-ST e PIS/COFINS monofásico já foram recolhidos antes da venda: a parcela \
         respectiva sai da alíquota do Simples e o preço sugerido fica mais baixo.",
    ));
    ui.add_space(Espaco::E8);
    let opcoes = [(0_u8, "Herdar"), (1, "Sim"), (2, "Não")];
    ui.columns(2, |c| {
        SeletorOpcao::novo("ICMS por substituição tributária", &mut estado.est.st)
            .opcoes(opcoes)
            .mostrar(&mut c[0]);
        SeletorOpcao::novo("PIS/COFINS monofásico", &mut estado.est.monofasico)
            .opcoes(opcoes)
            .mostrar(&mut c[1]);
    });
    ui.add_space(Espaco::E8);
    if ui.add(Botao::secundario("Salvar perfil fiscal")).clicked() {
        let r = motor.executar(
            sessao,
            "estoque.definir_perfil_fiscal_do_grupo.v1",
            &DefinirPerfilFiscalDoGrupo {
                grupo: no.id,
                icms_st: de_tri(estado.est.st.unwrap_or(0)),
                monofasico: de_tri(estado.est.monofasico.unwrap_or(0)),
            },
        );
        aplicar(ui.ctx(), motor, sessao, estado, r, "Perfil fiscal salvo");
    }
    ui.add_space(Espaco::E12);
}

fn bloco_fiscal(
    ui: &mut egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaEstoque,
    no: &NoArvoreGrupo,
) {
    SecaoExpansivel::nova("Perfil fiscal completo (origem, CST, CFOP, alíquotas, CEST)").mostrar(
        ui,
        |ui| {
            ui.add(Rotulo::apoio(
                "Vale para os produtos deste grupo e dos que estão abaixo, a menos que o produto \
                 ou um subgrupo defina o seu. Campo em branco herda do grupo acima.",
            ));
            ui.add_space(Espaco::E8);
            estado.est.fisc.mostrar(ui);
            ui.add_space(Espaco::E8);
            if ui
                .add(Botao::secundario("Salvar perfil fiscal completo"))
                .clicked()
            {
                match estado.est.fisc.para() {
                    Ok(campos) => {
                        let r = motor.executar(
                            sessao,
                            "estoque.definir_campos_fiscais_do_grupo.v1",
                            &mod_estoque::DefinirCamposFiscaisDoGrupo {
                                grupo: no.id,
                                campos,
                            },
                        );
                        aplicar(ui.ctx(), motor, sessao, estado, r, "Perfil fiscal salvo");
                    }
                    Err(m) => notificar(ui.ctx(), Notificacao::aviso(m)),
                }
            }
        },
    );
    ui.add_space(Espaco::E12);
}

fn bloco_mover(
    ui: &mut egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaEstoque,
    no: &NoArvoreGrupo,
) {
    ui.add(Rotulo::titulo_secao("Mover"));
    let destinos = estado.est.destinos_validos(no.id);
    if destinos.is_empty() {
        ui.add(Rotulo::apoio(
            "Não há para onde mover sem passar de quatro níveis.",
        ));
        ui.add_space(Espaco::E12);
        return;
    }
    ui.add(Rotulo::apoio(
        "O grupo vai com tudo que está dentro dele; só aparecem destinos que cabem na estrutura.",
    ));
    ui.add_space(Espaco::E8);
    SeletorOpcao::novo("Mover para", &mut estado.est.mover_para)
        .opcoes(destinos)
        .mostrar(ui);
    ui.add_space(Espaco::E8);
    if ui
        .add_enabled(estado.est.mover_para.is_some(), Botao::secundario("Mover"))
        .clicked()
    {
        let destino = estado.est.mover_para.filter(|d| *d != Id::NULO);
        let r = motor.executar(
            sessao,
            "estoque.mover_grupo_produto.v1",
            &MoverGrupoProduto {
                grupo: no.id,
                novo_pai: destino,
            },
        );
        aplicar(ui.ctx(), motor, sessao, estado, r, "Grupo movido");
    }
    ui.add_space(Espaco::E12);
}

fn formulario_novo(
    ui: &mut egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaEstoque,
    pai: Option<Id>,
) {
    ui.columns(2, |c| {
        c[0].add(Campo::novo("Código", &mut estado.est.novo_codigo).marcador("BEB"));
        c[1].add(Campo::novo("Nome", &mut estado.est.novo_nome));
    });
    ui.add_space(Espaco::E8);
    if ui.add(Botao::primario("Criar")).clicked() {
        match motor.executar(
            sessao,
            "estoque.criar_grupo_produto.v1",
            &CriarGrupoProduto {
                codigo: estado.est.novo_codigo.clone(),
                nome: estado.est.novo_nome.clone(),
                pai,
            },
        ) {
            Ok(g) => {
                estado.est.novo_codigo.clear();
                estado.est.novo_nome.clear();
                if let Some(p) = pai {
                    estado.est.recolhidos.remove(&p);
                }
                estado.carregar(motor, sessao);
                estado.est.selecionar(g.grupo_produto);
                notificar(ui.ctx(), Notificacao::sucesso("Grupo criado"));
            }
            Err(e) => notificar(ui.ctx(), Notificacao::erro(e.mensagem)),
        }
    }
}
