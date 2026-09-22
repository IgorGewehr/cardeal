//! Tela de Compras — notas de entrada + dialog (nova / ver / confirmar / vincular).
//! `docs/modulos/compras.md`.
//!
//! Fluxo: lançar nota manual (fornecedor + itens digitados) **ou** importar um XML já em mãos
//! (`rfd::FileDialog`, leitura local — ver [`importar_xml`]) → a cascata de casamento roda no
//! comando → itens não casados ganham busca de produto (`SeletorBusca`); sugestão forte ganha
//! aceite em um clique → quando `Conferida`, confirmar a entrada (baixa no estoque + título a
//! pagar, opcionalmente já baixado quando a compra foi paga na hora).
//!
//! Revisão de 2026-09-13 (mesma rodada de modernização de estoque/clientes/OS/financeiro):
//! busca + ordenação por coluna na lista, `Etiqueta` colorida por estado (nota e casamento de
//! item), diálogo de conferência decoupled do índice da lista (guarda a nota aberta por valor,
//! não por posição — evita apontar pra nota errada se a grade for reordenada com o diálogo
//! aberto, mesmo padrão que `tela_os.rs` já usa para `Dlg::Detalhe`).

use std::collections::HashMap;

use cardeal_cliente::{MotorLocal, SessaoLocal};
use cardeal_kernel::{Dinheiro, Id};
use cardeal_modkit::Icone;
use cardeal_ui::atoms::{Botao, Caixa, Divisor, Etiqueta, Rotulo, Tom, ValorDinheiro, ATALHO_NOVO};
use cardeal_ui::molecules::{
    dado, BarraFiltros, Campo, EstadoVazio, Mascara, OpcaoBusca, SecaoExpansivel, SeletorBusca,
    SeletorOpcao,
};
use cardeal_ui::organisms::{
    notificar, ColunaGrade, Dialogo, Direcao, Grade, LayoutTela, Notificacao, Ordenacao, Painel,
};
use cardeal_ui::tokens::{Espaco, TemaUi};
use eframe::egui;
use mod_clientes::{Papel, PessoasPorPapel};
use mod_compras::{
    ConfirmarEntrada, EstadoCasamento, EstadoNotaEntrada, ImportarNotaDeArquivoXml, ItemNota,
    ItemNotaEntrada, ItemNotaManual, ItensDaNota, LancarNotaManual, NotasRecentes,
    VincularProdutoManual,
};
use mod_estoque::{ItemLocal, ItemProdutoComSaldo, Locais, ProdutosComSaldo};

#[derive(Default, PartialEq)]
enum Dlg {
    #[default]
    Fechado,
    Ver,
    Nova,
}

#[derive(Default, Clone)]
struct LinhaItem {
    codigo: String,
    descricao: String,
    ncm: String,
    quantidade: String,
    valor_unitario: String,
}

#[derive(Default)]
struct FormNova {
    cnpj: String,
    nome: String,
    numero: String,
    serie: String,
    data: String,
    frete: String,
    itens: Vec<LinhaItem>,
}

/// Um `FormNova` pronto pra digitar: data de hoje, frete zerado, uma linha de item em branco.
fn form_nova_padrao() -> FormNova {
    FormNova {
        data: cardeal_kernel::Data::hoje(cardeal_kernel::Fuso::BRASILIA).to_string(),
        frete: "0,00".to_owned(),
        itens: vec![LinhaItem::default()],
        ..FormNova::default()
    }
}

/// Estado local da tela.
#[derive(Default)]
pub struct EstadoTelaCompras {
    notas: Vec<ItemNota>,
    fornecedores: HashMap<Id, String>,
    produtos: Vec<ItemProdutoComSaldo>,
    locais: Vec<ItemLocal>,
    itens_nota: Vec<ItemNotaEntrada>,
    /// A nota aberta no diálogo `Ver`, guardada por valor — não pelo índice em `notas`, que
    /// pode mudar de posição a qualquer quadro se o usuário ordenar a grade com o diálogo
    /// aberto (mesma decisão de `tela_os.rs::EstadoTelaOs::detalhe`).
    nota_aberta: Option<ItemNota>,
    local_sel: Option<Id>,
    /// Produto escolhido no `SeletorBusca` de cada item ainda não casado, por `item_nota`.
    vinc_produto: HashMap<Id, Option<Id>>,
    /// Texto digitado no `SeletorBusca` de cada item, por `item_nota`.
    busca_produto: HashMap<Id, String>,
    /// "Já foi pago" no diálogo de confirmar entrada — item 2 do briefing.
    pago_no_ato: bool,
    busca: String,
    ordenacao: Ordenacao,
    nova: FormNova,
    erro: Option<String>,
    dlg: Dlg,
}

impl EstadoTelaCompras {
    /// Recarrega notas, fornecedores, produtos e locais.
    pub fn carregar(&mut self, motor: &MotorLocal, sessao: &SessaoLocal) {
        match motor.consultar(sessao, "compras.notas_recentes.v1", &NotasRecentes) {
            Ok(n) => {
                self.notas = n;
                self.erro = None;
            }
            Err(e) => self.erro = Some(e.mensagem),
        }
        // A nota aberta no diálogo (se algum) precisa refletir o estado recém-recarregado —
        // senão "Confirmar entrada" mostraria o estado antigo até o usuário fechar e reabrir.
        if let Some(atual) = &self.nota_aberta {
            self.nota_aberta = self.notas.iter().find(|n| n.nota == atual.nota).cloned();
        }
        if let Ok(f) = motor.consultar(
            sessao,
            "clientes.pessoas_por_papel.v1",
            &PessoasPorPapel {
                papel: Papel::Fornecedor,
                busca: None,
            },
        ) {
            self.fornecedores = f.into_iter().map(|p| (p.pessoa, p.nome)).collect();
        }
        if let Ok(p) = motor.consultar(sessao, "estoque.produtos_com_saldo.v1", &ProdutosComSaldo) {
            self.produtos = p;
        }
        if let Ok(l) = motor.consultar(sessao, "estoque.locais.v1", &Locais) {
            self.locais = l;
        }
    }

    fn abrir_nota(&mut self, motor: &MotorLocal, sessao: &SessaoLocal, nota: Id) {
        let Some(item) = self.notas.iter().find(|n| n.nota == nota).cloned() else {
            return;
        };
        self.itens_nota = motor
            .consultar(sessao, "compras.itens_da_nota.v1", &ItensDaNota { nota })
            .unwrap_or_default();
        self.vinc_produto.clear();
        self.busca_produto.clear();
        self.pago_no_ato = false;
        self.nota_aberta = Some(item);
        self.dlg = Dlg::Ver;
    }

    fn recarregar_itens(&mut self, motor: &MotorLocal, sessao: &SessaoLocal, nota: Id) {
        self.itens_nota = motor
            .consultar(sessao, "compras.itens_da_nota.v1", &ItensDaNota { nota })
            .unwrap_or_default();
    }

    fn nome(&self, fornecedor: Id) -> String {
        self.fornecedores
            .get(&fornecedor)
            .cloned()
            .unwrap_or_else(|| "—".to_owned())
    }
}

/// Desenha a tela inteira.
pub fn mostrar(
    ui: &mut egui::Ui,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaCompras,
) {
    LayoutTela::nova("Compras").mostrar(
        ui,
        estado,
        |ui, estado| {
            if ui
                .add(Botao::primario("+ Nota manual").tecla(ATALHO_NOVO))
                .clicked()
            {
                estado.nova = form_nova_padrao();
                estado.dlg = Dlg::Nova;
            }
            if ui.add(Botao::secundario("Importar XML")).clicked() {
                importar_xml(ui.ctx(), motor, sessao, estado);
            }
            if ui.add(Botao::secundario("Importar pasta")).clicked() {
                importar_pasta_xml(ui.ctx(), motor, sessao, estado);
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
            lista(ui, motor, sessao, estado);
        },
    );

    match estado.dlg {
        Dlg::Fechado => {}
        Dlg::Ver => dialogo_ver(ui.ctx(), motor, sessao, estado),
        Dlg::Nova => dialogo_nova(ui.ctx(), motor, sessao, estado),
    }
}

/// Abre o seletor de arquivo do sistema (multi-seleção) e importa cada `.xml` escolhido —
/// pedido explícito do usuário (2026-09-15): a entrega de um fornecedor quase sempre traz
/// mais de uma nota, e repetir "escolher arquivo → conferir → fechar" uma vez por XML era a
/// burocracia que sobrava na importação. `importar_varios_xml` faz o trabalho de verdade.
fn importar_xml(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaCompras,
) {
    let caminhos = rfd::FileDialog::new()
        .add_filter("XML de nota fiscal", &["xml"])
        .pick_files()
        .unwrap_or_default();
    importar_varios_xml(ctx, motor, sessao, estado, caminhos);
}

/// Abre o seletor de pasta do sistema e importa todo `.xml` que estiver direto nela (sem
/// descer em subpastas) — pra quando o fornecedor/contador larga várias notas na mesma pasta
/// e o balcão só quer apontar pra ela de vez em quando, sem escolher arquivo por arquivo.
/// Reimportar uma pasta já processada não duplica nada (mesma idempotência por chave de
/// acesso de sempre) — clicar de novo depois que chegam notas novas é seguro.
fn importar_pasta_xml(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaCompras,
) {
    let Some(pasta) = rfd::FileDialog::new().pick_folder() else {
        return;
    };
    let caminhos: Vec<std::path::PathBuf> = match std::fs::read_dir(&pasta) {
        Ok(entradas) => entradas
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| {
                p.is_file()
                    && p.extension()
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("xml"))
            })
            .collect(),
        Err(e) => {
            notificar(
                ctx,
                Notificacao::erro(format!("Não foi possível abrir a pasta: {e}")),
            );
            return;
        }
    };
    if caminhos.is_empty() {
        notificar(
            ctx,
            Notificacao::aviso("Nenhum .xml encontrado nessa pasta."),
        );
        return;
    }
    importar_varios_xml(ctx, motor, sessao, estado, caminhos);
}

/// O miolo comum de importar N arquivos XML de uma vez: lê cada um, chama
/// `compras.importar_nota_de_arquivo_xml.v1` (`mod_compras::ImportarNotaDeArquivoXml`) — a
/// leitura do arquivo em si é responsabilidade desta tela; o comando só recebe o texto já em
/// UTF-8 e roda a mesma cascata de casamento/rateio que a nota manual usa. Idempotente por
/// chave de acesso: reimportar o mesmo arquivo não duplica, só reabre a nota existente — por
/// isso é seguro chamar isto de novo sobre uma seleção/pasta que já tinha sido importada
/// antes (só as notas novas de fato entram). Com um único arquivo, mantém o comportamento de
/// antes: abre o dialog "Ver" da nota direto. Com vários, só mostra o resumo — abrir N
/// diálogos em sequência seria pior que deixar o balcão escolher da lista.
fn importar_varios_xml(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaCompras,
    caminhos: Vec<std::path::PathBuf>,
) {
    if caminhos.is_empty() {
        return;
    }
    let total = caminhos.len();
    let mut importadas = 0usize;
    let mut precisam_revisao = 0usize;
    let mut erros: Vec<String> = Vec::new();
    let mut ultima_nota = None;

    for caminho in &caminhos {
        let nome = caminho.file_name().map_or_else(
            || caminho.to_string_lossy().into_owned(),
            |n| n.to_string_lossy().into_owned(),
        );
        let xml = match std::fs::read_to_string(caminho) {
            Ok(c) => c,
            Err(e) => {
                erros.push(format!("{nome}: não foi possível ler ({e})"));
                continue;
            }
        };
        if xml.trim().is_empty() {
            erros.push(format!("{nome}: arquivo vazio"));
            continue;
        }
        match motor.executar(
            sessao,
            "compras.importar_nota_de_arquivo_xml.v1",
            &ImportarNotaDeArquivoXml { xml },
        ) {
            Ok(relatorio) => {
                importadas += 1;
                if relatorio.itens_nao_casados > 0 {
                    precisam_revisao += 1;
                }
                ultima_nota = Some(relatorio.nota_entrada);
            }
            Err(e) => erros.push(format!("{nome}: {}", e.mensagem)),
        }
    }

    estado.carregar(motor, sessao);

    if total == 1 {
        if let Some(nota) = ultima_nota {
            estado.abrir_nota(motor, sessao, nota);
        }
        if let Some(erro) = erros.first() {
            notificar(ctx, Notificacao::erro(erro.clone()));
            return;
        }
        let msg = if precisam_revisao == 0 {
            "Nota importada — todos os itens já casaram."
        } else {
            "Nota importada — confira o casamento dos itens."
        };
        notificar(ctx, Notificacao::sucesso(msg));
        return;
    }

    let titulo = format!("{importadas} de {total} nota(s) importada(s)");
    let mut n = if erros.is_empty() {
        Notificacao::sucesso(titulo)
    } else {
        Notificacao::aviso(titulo)
    };
    let mut detalhe = Vec::new();
    if precisam_revisao > 0 {
        detalhe.push(format!(
            "{precisam_revisao} precisam de revisão do casamento de item"
        ));
    }
    if !erros.is_empty() {
        detalhe.push(format!("{} com erro:", erros.len()));
        detalhe.extend(erros);
    }
    if !detalhe.is_empty() {
        n = n.detalhe(detalhe.join("\n"));
    }
    notificar(ctx, n);
}

fn lista(
    ui: &mut egui::Ui,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaCompras,
) {
    if estado.notas.is_empty() {
        if estado.erro.is_none()
            && EstadoVazio::novo(Icone::Nota, "Nenhuma nota de entrada.")
                .acao("Lançar a primeira")
                .mostrar(ui)
        {
            estado.nova = form_nova_padrao();
            estado.dlg = Dlg::Nova;
        }
        return;
    }

    BarraFiltros::nova(&mut estado.busca)
        .marcador("Buscar por fornecedor ou número")
        .mostrar(ui);

    let termo = estado.busca.trim().to_lowercase();
    let indices: Vec<usize> = (0..estado.notas.len())
        .filter(|&i| {
            if termo.is_empty() {
                return true;
            }
            let n = &estado.notas[i];
            estado.nome(n.fornecedor).to_lowercase().contains(&termo)
                || n.numero.to_lowercase().contains(&termo)
                || n.serie.to_lowercase().contains(&termo)
        })
        .collect();
    let colunas = vec![
        ColunaGrade::nova("Fornecedor"),
        ColunaGrade::nova("Nº / série").largura(120.0),
        ColunaGrade::nova("Emissão").largura(120.0),
        ColunaGrade::nova("Itens").largura(70.0).numero(),
        ColunaGrade::nova("Total").largura(130.0).numero(),
        ColunaGrade::nova("Estado").largura(140.0),
    ];
    let resposta = Grade::nova(colunas)
        .selecionavel(None)
        .ordenacao(estado.ordenacao.atual())
        .vazio("Nenhuma nota para essa busca.")
        .mostrar(ui, indices.len(), |i, row| {
            let n = &estado.notas[indices[i]];
            row.col(|ui| {
                ui.add(Rotulo::interface(estado.nome(n.fornecedor)));
            });
            row.col(|ui| {
                ui.add(Rotulo::interface(format!("{} / {}", n.numero, n.serie)));
            });
            row.col(|ui| {
                ui.add(Rotulo::interface(n.data_emissao.to_string()));
            });
            row.col(|ui| {
                ui.add(Rotulo::interface(n.itens.to_string()));
            });
            row.col(|ui| {
                ui.add(ValorDinheiro::novo(n.valor_total));
            });
            row.col(|ui| {
                let (rotulo, tom) = estado_nota_etiqueta(n.estado);
                ui.add(Etiqueta::nova(rotulo, tom));
            });
        });

    if let Some((coluna, direcao)) = estado.ordenacao.clicar(&resposta) {
        ordenar_notas(&mut estado.notas, &estado.fornecedores, coluna, direcao);
    }
    if let Some(i) = resposta.linha_clicada {
        let id = estado.notas[indices[i]].nota;
        estado.abrir_nota(motor, sessao, id);
    }
}

/// Rótulo em português + tom da etiqueta de estado da nota, para a `Grade` e o diálogo.
const fn estado_nota_etiqueta(e: EstadoNotaEntrada) -> (&'static str, Tom) {
    match e {
        EstadoNotaEntrada::AConferir => ("A conferir", Tom::Atencao),
        EstadoNotaEntrada::Conferida => ("Conferida", Tom::Info),
        EstadoNotaEntrada::Confirmada => ("Confirmada", Tom::Positivo),
        EstadoNotaEntrada::Devolvida => ("Devolvida", Tom::Negativo),
    }
}

/// Rótulo em português + tom da etiqueta de casamento de um item, para o diálogo de
/// conferência.
const fn estado_casamento_etiqueta(e: EstadoCasamento) -> (&'static str, Tom) {
    match e {
        EstadoCasamento::Casado => ("Casado", Tom::Positivo),
        EstadoCasamento::SugestaoForte => ("Sugestão", Tom::Atencao),
        EstadoCasamento::NaoCasado => ("Não casado", Tom::Negativo),
    }
}

/// Ordena `notas` pela coluna clicada no cabeçalho da [`Grade`] (Fornecedor, Nº/série,
/// Emissão, Itens, Total, Estado).
fn ordenar_notas(
    notas: &mut [ItemNota],
    fornecedores: &HashMap<Id, String>,
    coluna: usize,
    direcao: Direcao,
) {
    let nome = |f: Id| fornecedores.get(&f).map_or("—", String::as_str);
    notas.sort_by(|a, b| {
        let ordem = match coluna {
            0 => nome(a.fornecedor).cmp(nome(b.fornecedor)),
            1 => (a.numero.as_str(), a.serie.as_str()).cmp(&(b.numero.as_str(), b.serie.as_str())),
            2 => a.data_emissao.cmp(&b.data_emissao),
            3 => a.itens.cmp(&b.itens),
            4 => a.valor_total.cmp(&b.valor_total),
            5 => estado_nota_etiqueta(a.estado)
                .0
                .cmp(estado_nota_etiqueta(b.estado).0),
            _ => std::cmp::Ordering::Equal,
        };
        match direcao {
            Direcao::Ascendente => ordem,
            Direcao::Descendente => ordem.reverse(),
        }
    });
}

fn dialogo_nova(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaCompras,
) {
    let fechar = Dialogo::nova("Nova nota de entrada")
        .largura(760.0)
        .mostrar(
            ctx,
            estado,
            |ui, estado| {
                let f = &mut estado.nova;
                ui.columns(2, |c| {
                    c[0].add(
                        Campo::novo("CNPJ do fornecedor", &mut f.cnpj)
                            .mascara(Mascara::Documento)
                            .marcador("00.000.000/0000-00"),
                    );
                    c[1].add(Campo::novo("Razão social", &mut f.nome));
                });
                ui.add_space(Espaco::E8);
                // CNPJ/razão social são obrigatórios de verdade (`lancar`, abaixo, valida o
                // CNPJ e `LancarNotaManual::executar` resolve o fornecedor por ele — não dá
                // pra lançar nota sem saber de quem). Número/série/NCM/código do fornecedor
                // são só texto livre no domínio (`ItemNotaManual`/`LancarNotaManual`) — a
                // cascata de casamento usa o NCM quando ele existe, mas não exige.
                ui.columns(3, |c| {
                    c[0].add(Campo::novo("Número (opcional)", &mut f.numero));
                    c[1].add(Campo::novo("Série (opcional)", &mut f.serie));
                    c[2].add(Campo::novo("Emissão", &mut f.data).mascara(Mascara::Data));
                });
                ui.add_space(Espaco::E8);
                ui.add(Campo::novo("Frete (opcional)", &mut f.frete).marcador("0,00"));

                ui.add_space(Espaco::E12);
                ui.add(Rotulo::titulo_secao("Itens"));
                ui.add_space(Espaco::E4);
                let mut remover = None;
                let n_itens = f.itens.len();
                for (idx, it) in f.itens.iter_mut().enumerate() {
                    ui.columns(5, |c| {
                        c[0].add(Campo::novo("Código (opcional)", &mut it.codigo));
                        c[1].add(Campo::novo("Descrição", &mut it.descricao));
                        c[2].add(Campo::novo("NCM (opcional)", &mut it.ncm));
                        c[3].add(Campo::novo("Qtd", &mut it.quantidade));
                        c[4].add(Campo::novo("Vlr unit.", &mut it.valor_unitario));
                    });
                    if n_itens > 1 && ui.add(Botao::fantasma("remover linha")).clicked() {
                        remover = Some(idx);
                    }
                    ui.add_space(Espaco::E8);
                }
                if let Some(idx) = remover {
                    f.itens.remove(idx);
                }
                if ui.add(Botao::secundario("+ Linha")).clicked() {
                    f.itens.push(LinhaItem::default());
                }
            },
            |ui, estado| {
                if ui.add(Botao::primario("Lançar nota")).clicked() {
                    lancar(ui.ctx(), motor, sessao, estado);
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

fn lancar(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaCompras,
) {
    let f = &estado.nova;
    let Ok(data_emissao) = f.data.parse() else {
        notificar(
            ctx,
            Notificacao::aviso("Data de emissão inválida (dd/mm/aaaa)."),
        );
        return;
    };
    let frete = f.frete.parse().unwrap_or(Dinheiro::ZERO);
    let mut itens = Vec::new();
    for it in &f.itens {
        if it.descricao.trim().is_empty() {
            continue;
        }
        let (Ok(quantidade), Ok(valor_unitario)) =
            (it.quantidade.parse(), it.valor_unitario.parse())
        else {
            notificar(
                ctx,
                Notificacao::aviso(format!("Item \"{}\": qtd/valor inválidos.", it.descricao)),
            );
            return;
        };
        itens.push(ItemNotaManual {
            codigo_fornecedor: it.codigo.clone(),
            descricao: it.descricao.clone(),
            ncm: it.ncm.clone(),
            // Sem campo de GTIN na nota digitada à mão ainda — a cascata de casamento cai
            // pra regra aprendida/NCM (comportamento de antes deste campo existir).
            codigo_barras: None,
            quantidade,
            valor_unitario,
        });
    }
    if itens.is_empty() {
        notificar(ctx, Notificacao::aviso("Adicione ao menos um item."));
        return;
    }

    match motor.executar(
        sessao,
        "compras.lancar_nota_manual.v1",
        &LancarNotaManual {
            fornecedor_cnpj: f.cnpj.clone(),
            fornecedor_nome: f.nome.clone(),
            numero: f.numero.clone(),
            serie: f.serie.clone(),
            data_emissao,
            itens,
            valor_frete: frete,
            valor_seguro: Dinheiro::ZERO,
            valor_outras_despesas: Dinheiro::ZERO,
        },
    ) {
        Ok(_) => {
            estado.dlg = Dlg::Fechado;
            estado.carregar(motor, sessao);
            notificar(ctx, Notificacao::sucesso("Nota lançada"));
        }
        Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
    }
}

fn dialogo_ver(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaCompras,
) {
    let Some(n) = estado.nota_aberta.clone() else {
        estado.dlg = Dlg::Fechado;
        return;
    };
    let nome = estado.nome(n.fornecedor);
    let ops_local: Vec<(Id, String)> = estado
        .locais
        .iter()
        .map(|l| (l.id, l.nome.clone()))
        .collect();

    let fechar = Dialogo::nova(format!("Nota {} · {nome}", n.numero))
        .largura(760.0)
        .mostrar(
            ctx,
            estado,
            |ui, estado| {
                ui.columns(2, |c| {
                    dado(&mut c[0], "Série", &n.serie);
                    dado(&mut c[1], "Emissão", &n.data_emissao.to_string());
                });
                ui.columns(2, |c| {
                    dado(&mut c[0], "Total", &n.valor_total.formatar_com_simbolo());
                    dado(&mut c[1], "Estado", estado_nota_etiqueta(n.estado).0);
                });

                ui.add_space(Espaco::E12);
                ui.add(Rotulo::titulo_secao("Itens e casamento"));
                ui.add_space(Espaco::E4);

                let nomes_produto: HashMap<Id, String> = estado
                    .produtos
                    .iter()
                    .map(|p| (p.produto, p.nome.clone()))
                    .collect();
                let mut vincular: Option<(Id, Id)> = None;
                for it in estado.itens_nota.clone() {
                    let opcoes = opcoes_busca_produtos(&estado.produtos);
                    let busca = estado.busca_produto.entry(it.id).or_default();
                    let sel = estado.vinc_produto.entry(it.id).or_default();
                    if let Some(produto) =
                        linha_item_casamento(ui, &it, &nomes_produto, opcoes, busca, sel)
                    {
                        vincular = Some((it.id, produto));
                    }
                    ui.add_space(Espaco::E8);
                }
                if let Some((item_nota, produto)) = vincular {
                    vincular_item(ui.ctx(), motor, sessao, estado, n.nota, item_nota, produto);
                }

                if matches!(n.estado, EstadoNotaEntrada::Conferida) {
                    ui.add_space(Espaco::E12);
                    ui.add(Divisor::novo());
                    ui.add_space(Espaco::E12);
                    ui.add(Rotulo::titulo_secao("Confirmar entrada"));
                    ui.add_space(Espaco::E8);
                    SeletorOpcao::novo("Local que recebe", &mut estado.local_sel)
                        .opcoes(ops_local.clone())
                        .mostrar(ui);
                    ui.add_space(Espaco::E8);
                    ui.add(Caixa::nova(
                        &mut estado.pago_no_ato,
                        "Nota já foi paga — baixar o título a pagar automaticamente",
                    ));
                    if estado.pago_no_ato {
                        ui.add(
                            Rotulo::campo("O título nasce quitado em vez de pendente.")
                                .cor(ui.cores().positivo),
                        );
                    }
                    ui.add_space(Espaco::E8);
                    if ui.add(Botao::primario("Confirmar entrada")).clicked() {
                        confirmar(ui.ctx(), motor, sessao, estado, n.nota);
                    }
                }
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

/// As opções de produto para um `SeletorBusca` — reconstruída a cada item porque
/// `OpcaoBusca` não é `Clone` (o vetor é consumido por `.opcoes()`).
fn opcoes_busca_produtos(produtos: &[ItemProdutoComSaldo]) -> Vec<OpcaoBusca<Id>> {
    produtos
        .iter()
        .map(|p| OpcaoBusca::nova(p.produto, p.nome.clone()))
        .collect()
}

/// Uma linha de item no diálogo de conferência/casamento — item 4 do briefing: item não
/// casado precisa de destaque claro e ação óbvia (`SeletorBusca`, não um combo cru); sugestão
/// forte (`EstadoCasamento::SugestaoForte`, casamento ≥ `LIMIAR_SUGESTAO_FORTE` = 82%) precisa
/// de um jeito rápido de aceitar com um clique.
///
/// Devolve o produto a vincular quando alguma ação (aceitar sugestão, ou escolher e vincular)
/// disparou neste quadro.
fn linha_item_casamento(
    ui: &mut egui::Ui,
    item: &ItemNotaEntrada,
    nomes_produto: &HashMap<Id, String>,
    opcoes: Vec<OpcaoBusca<Id>>,
    busca: &mut String,
    selecionado: &mut Option<Id>,
) -> Option<Id> {
    let cores = ui.cores();
    let mut vincular = None;
    // O estado do casamento define o tom do painel: sem produto = vermelho, sugestão = âmbar,
    // casado = neutro.
    let painel = match item.estado_casamento {
        EstadoCasamento::NaoCasado => Painel::novo().realce(Tom::Negativo),
        EstadoCasamento::SugestaoForte => Painel::novo().realce(Tom::Atencao),
        EstadoCasamento::Casado => Painel::novo().plano(),
    }
    .compacto();

    painel.mostrar(ui, |ui| {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.add(Rotulo::interface(format!(
                    "{} × {}",
                    item.quantidade, item.descricao_fornecedor
                )));
                if let (EstadoCasamento::SugestaoForte, Some(produto)) =
                    (item.estado_casamento, item.produto_casado)
                {
                    let nome = nomes_produto
                        .get(&produto)
                        .map_or("produto do estoque", String::as_str);
                    ui.add(Rotulo::campo(format!("Sugestão: {nome}")));
                }
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let (rotulo, tom) = estado_casamento_etiqueta(item.estado_casamento);
                ui.add(Etiqueta::nova(rotulo, tom));
            });
        });

        match item.estado_casamento {
            EstadoCasamento::Casado => {}
            EstadoCasamento::SugestaoForte => {
                ui.add_space(Espaco::E8);
                if ui
                    .add(Botao::primario("Aceitar sugestão").pequeno())
                    .clicked()
                {
                    vincular = item.produto_casado;
                }
                ui.add_space(Espaco::E4);
                SecaoExpansivel::nova("Prefere outro produto?").mostrar(ui, |ui| {
                    SeletorBusca::novo("Produto", busca, selecionado)
                        .opcoes(opcoes)
                        .marcador("Buscar produto por nome…")
                        .mostrar(ui);
                    if selecionado.is_some()
                        && ui
                            .add(Botao::secundario("Vincular este").pequeno())
                            .clicked()
                    {
                        vincular = *selecionado;
                    }
                });
            }
            EstadoCasamento::NaoCasado => {
                ui.add_space(Espaco::E8);
                ui.add(Rotulo::campo("Sem produto vinculado — busque abaixo.").cor(cores.negativo));
                ui.add_space(Espaco::E4);
                SeletorBusca::novo("Produto", busca, selecionado)
                    .opcoes(opcoes)
                    .marcador("Buscar produto por nome…")
                    .mostrar(ui);
                if selecionado.is_some() {
                    ui.add_space(Espaco::E4);
                    if ui.add(Botao::primario("Vincular").pequeno()).clicked() {
                        vincular = *selecionado;
                    }
                }
            }
        }
    });

    vincular
}

fn vincular_item(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaCompras,
    nota: Id,
    item_nota: Id,
    produto: Id,
) {
    match motor.executar(
        sessao,
        "compras.vincular_produto_manual.v1",
        &VincularProdutoManual { item_nota, produto },
    ) {
        Ok(_) => {
            estado.vinc_produto.remove(&item_nota);
            estado.busca_produto.remove(&item_nota);
            estado.recarregar_itens(motor, sessao, nota);
            estado.carregar(motor, sessao);
            notificar(ctx, Notificacao::sucesso("Produto vinculado"));
        }
        Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
    }
}

fn confirmar(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaCompras,
    nota: Id,
) {
    let Some(local) = estado.local_sel else {
        notificar(
            ctx,
            Notificacao::aviso("Escolha o local de estoque que recebe."),
        );
        return;
    };
    match motor.executar(
        sessao,
        "compras.confirmar_entrada.v1",
        &ConfirmarEntrada {
            nota_entrada: nota,
            local,
            gerar_titulo_a_pagar: None,
            pago_no_ato: Some(estado.pago_no_ato),
        },
    ) {
        Ok(saida) => {
            estado.dlg = Dlg::Fechado;
            estado.carregar(motor, sessao);
            let msg = if saida.pago {
                "Entrada confirmada — título já baixado (pago no ato)"
            } else {
                "Entrada confirmada"
            };
            notificar(ctx, Notificacao::sucesso(msg));
        }
        Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
    }
}
