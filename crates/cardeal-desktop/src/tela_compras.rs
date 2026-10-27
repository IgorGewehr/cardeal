//! Tela de Compras — notas de entrada + dialog (nova / ver / confirmar / vincular).
//! `docs/modulos/compras.md`.
//!
//! Fluxo: lançar nota manual (fornecedor + itens digitados) **ou** importar um XML já em mãos
//! (`rfd::FileDialog`, leitura local — ver [`importar_xml`]) → a cascata de casamento roda no
//! comando → itens não casados ganham busca de produto (`SeletorBusca`); sugestão forte ganha
//! aceite em um clique → quando `Conferida`, confirmar a entrada (baixa no estoque + título a
//! pagar, opcionalmente já baixado quando a compra foi paga na hora).
//!
//! As notas que a SEFAZ entrega ao CNPJ (distribuição `DFe`) chegam sozinhas: a busca roda em
//! segundo plano (`crate::compras_fundo`) e o botão "Buscar na SEFAZ" a dispara na hora. A
//! seção "Notas que chegaram pela SEFAZ" mostra a última busca e as que pedem atenção (à espera
//! da SEFAZ liberar a nota completa, as que não importaram, as ignoradas e por quê).
//!
//! Revisão de 2026-09-13 (mesma rodada de modernização de estoque/clientes/OS/financeiro):
//! busca + ordenação por coluna na lista, `Etiqueta` colorida por estado (nota e casamento de
//! item), diálogo de conferência decoupled do índice da lista (guarda a nota aberta por valor,
//! não por posição — evita apontar pra nota errada se a grade for reordenada com o diálogo
//! aberto, mesmo padrão que `tela_os.rs` já usa para `Dlg::Detalhe`).

use std::collections::HashMap;

use cardeal_cliente::{Motor, Sessao};
use cardeal_kernel::{Dinheiro, Fuso, Id};
use cardeal_modkit::Icone;
use cardeal_ui::atoms::{Botao, Caixa, Divisor, Etiqueta, Rotulo, Tom, ValorDinheiro, ATALHO_NOVO};
use cardeal_ui::molecules::{
    dado, BarraFiltros, Campo, EstadoVazio, Mascara, OpcaoBusca, SecaoExpansivel, SeletorBusca,
    SeletorOpcao,
};
use cardeal_ui::organisms::{
    notificar, ColunaGrade, Dialogo, Direcao, EstadoSeletorArvore, Grade, LayoutTela, NoArvore,
    Notificacao, Ordenacao, Painel, SeletorEmArvore,
};
use cardeal_ui::tokens::{Espaco, TemaUi};
use eframe::egui;
use mod_clientes::{Papel, PessoasPorPapel};
use mod_compras::{
    ConfirmarEntrada, CriarProdutoDoItem, EstadoCasamento, EstadoNaSefaz, EstadoNotaEntrada,
    ImportarDeNovoDaSefaz, ImportarNotaDeArquivoXml, ItemNota, ItemNotaEntrada, ItemNotaManual,
    ItensDaNota, LancarNotaManual, NotasNaSefaz, NotasRecentes, ProdutoDoItemCriado,
    RelatorioImportacao, SituacaoDaBusca, VincularProdutoManual,
};
use mod_estoque::{
    ArvoreGruposProduto, ItemLocal, ItemProdutoComSaldo, Locais, ProdutosComSaldo,
    SugerirNomeDoProduto,
};

mod devolucao;
mod estorno;
mod parcelas;

/// O fuso das datas e horas da tela.
const FUSO: Fuso = Fuso::BRASILIA;

/// O formulário "Criar produto novo" de um item não casado: o nome (começa com a sugestão
/// padronizada pelo dicionário) e o grupo opcional.
#[derive(Default)]
struct FormProdutoNovo {
    nome: String,
    grupo: Option<Id>,
    seletor: EstadoSeletorArvore,
}

/// O que o usuário pediu na linha de um item.
enum AcaoItem {
    Vincular(Id),
    Criar,
}

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
    /// "Criar produto novo" de cada item não casado, por `item_nota`.
    produto_novo: HashMap<Id, FormProdutoNovo>,
    /// A estrutura mercadológica, para escolher o grupo do produto novo.
    grupos: Vec<NoArvore>,
    /// "Já foi pago" no diálogo de confirmar entrada — item 2 do briefing.
    pago_no_ato: bool,
    /// As parcelas da conta a pagar da nota aberta.
    parcelas: parcelas::EditorParcelas,
    /// O estorno da entrada da nota aberta (confirmada).
    estorno: estorno::EstadoEstorno,
    /// As devoluções ao fornecedor da nota aberta.
    devolucao: devolucao::EstadoDevolucao,
    busca: String,
    ordenacao: Ordenacao,
    nova: FormNova,
    erro: Option<String>,
    dlg: Dlg,
    /// A busca de notas na SEFAZ: a última rodada e as notas que pedem atenção.
    sefaz: Option<SituacaoDaBusca>,
    /// A rodada da busca que a tela já mostrou (`crate::compras_fundo::geracao`).
    geracao_sefaz: u64,
    /// O usuário pediu a busca: avisa o resultado quando a rodada terminar.
    busca_pedida: bool,
}

impl EstadoTelaCompras {
    /// Recarrega notas, fornecedores, produtos e locais.
    pub fn carregar(&mut self, motor: &Motor, sessao: &Sessao) {
        self.geracao_sefaz = crate::compras_fundo::geracao();
        self.sefaz = motor
            .consultar(sessao, "compras.notas_na_sefaz.v1", &NotasNaSefaz)
            .ok();
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
        if let Ok(nos) = motor.consultar(
            sessao,
            "estoque.arvore_grupos_produto.v1",
            &ArvoreGruposProduto,
        ) {
            self.grupos = nos
                .into_iter()
                .map(|n| NoArvore {
                    id: n.id,
                    nivel: n.nivel,
                    rotulo: n.nome,
                    apoio: None,
                    tem_filhos: false,
                })
                .collect();
        }
    }

    /// O formulário do item, com o nome sugerido na primeira vez.
    fn form_produto_novo(
        &mut self,
        motor: &Motor,
        sessao: &Sessao,
        item: &ItemNotaEntrada,
    ) -> &mut FormProdutoNovo {
        self.produto_novo
            .entry(item.id)
            .or_insert_with(|| FormProdutoNovo {
                nome: motor
                    .consultar(
                        sessao,
                        "estoque.sugerir_nome_do_produto.v1",
                        &SugerirNomeDoProduto {
                            descricao: item.descricao_fornecedor.clone(),
                        },
                    )
                    .unwrap_or_else(|_| item.descricao_fornecedor.clone()),
                ..FormProdutoNovo::default()
            })
    }

    fn abrir_nota(&mut self, motor: &Motor, sessao: &Sessao, nota: Id) {
        let Some(item) = self.notas.iter().find(|n| n.nota == nota).cloned() else {
            return;
        };
        self.itens_nota = motor
            .consultar(sessao, "compras.itens_da_nota.v1", &ItensDaNota { nota })
            .unwrap_or_default();
        self.vinc_produto.clear();
        self.busca_produto.clear();
        self.pago_no_ato = false;
        self.parcelas = parcelas::EditorParcelas::carregar(motor, sessao, &item);
        self.estorno = estorno::EstadoEstorno::default();
        self.devolucao = devolucao::EstadoDevolucao::default();
        self.nota_aberta = Some(item);
        self.dlg = Dlg::Ver;
    }

    fn recarregar_itens(&mut self, motor: &Motor, sessao: &Sessao, nota: Id) {
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
pub fn mostrar(ui: &mut egui::Ui, motor: &Motor, sessao: &Sessao, estado: &mut EstadoTelaCompras) {
    // A busca na SEFAZ terminou uma rodada: as notas novas aparecem; quem pediu, sabe o que deu.
    if estado.geracao_sefaz != crate::compras_fundo::geracao() {
        estado.carregar(motor, sessao);
        if std::mem::take(&mut estado.busca_pedida) {
            match crate::compras_fundo::ultimo_resultado() {
                Some(Ok(r)) => notificar(ui.ctx(), Notificacao::sucesso(format!("SEFAZ: {r}"))),
                Some(Err(e)) => {
                    notificar(ui.ctx(), Notificacao::erro(format!("Busca na SEFAZ: {e}")))
                }
                None => {}
            }
        }
    }
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
            if sessao.concede("compras.entrada.importar") {
                let rodando = crate::compras_fundo::rodando();
                if ui
                    .add(
                        Botao::secundario("Buscar na SEFAZ")
                            .carregando(rodando)
                            .habilitado(!rodando),
                    )
                    .clicked()
                    && crate::compras_fundo::disparar(ui.ctx(), motor, sessao)
                {
                    estado.busca_pedida = true;
                }
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
            notas_na_sefaz(ui, motor, sessao, estado);
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
    motor: &Motor,
    sessao: &Sessao,
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
    motor: &Motor,
    sessao: &Sessao,
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
    motor: &Motor,
    sessao: &Sessao,
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

fn lista(ui: &mut egui::Ui, motor: &Motor, sessao: &Sessao, estado: &mut EstadoTelaCompras) {
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
                ui.horizontal(|ui| {
                    let (rotulo, tom) = estado_nota_etiqueta(n.estado);
                    ui.add(Etiqueta::nova(rotulo, tom));
                    if n.alerta.is_some() && n.estado == EstadoNotaEntrada::AConferir {
                        ui.add(Etiqueta::atencao("ver aviso"));
                    }
                });
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

/// A seção das notas que chegaram pela SEFAZ: a última busca e as notas que pedem atenção.
/// Some enquanto a empresa nunca buscou.
fn notas_na_sefaz(
    ui: &mut egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaCompras,
) {
    let Some(s) = estado.sefaz.as_ref() else {
        return;
    };
    if s.ultima_consulta.is_none() && s.notas.is_empty() {
        return;
    }
    let pedem_atencao = s
        .notas
        .iter()
        .filter(|n| matches!(n.estado, EstadoNaSefaz::Aguardando | EstadoNaSefaz::Erro))
        .count();
    let pode_importar = sessao.concede("compras.entrada.importar");
    let mut de_novo: Option<String> = None;
    SecaoExpansivel::nova("Notas que chegaram pela SEFAZ")
        .aberta_por_padrao(pedem_atencao > 0)
        .mostrar(ui, |ui| {
            if let Some(quando) = s.ultima_consulta {
                ui.add(
                    Rotulo::campo(format!(
                        "Última busca: {} — {}",
                        quando.formatar(FUSO),
                        s.ultimo_resultado.as_deref().unwrap_or("sem resumo")
                    ))
                    .quebravel(),
                );
            }
            if let Some(p) = s.proxima_consulta {
                ui.add(Rotulo::campo(format!(
                    "A SEFAZ aceita a próxima busca a partir das {}.",
                    p.hora(FUSO).formatar()
                )));
            }
            if s.notas.is_empty() {
                return;
            }
            ui.add_space(Espaco::E8);
            Grade::nova(vec![
                ColunaGrade::nova("Fornecedor"),
                ColunaGrade::nova("Emissão").largura(110.0),
                ColunaGrade::nova("Valor").largura(120.0).numero(),
                ColunaGrade::nova("Situação").largura(170.0),
                ColunaGrade::nova("Por quê"),
                ColunaGrade::nova("").largura(160.0),
            ])
            .com_acoes()
            .id_salt("compras-notas-na-sefaz")
            .mostrar(ui, s.notas.len(), |i, row| {
                let n = &s.notas[i];
                row.col(|ui| {
                    let nome = if n.emitente.is_empty() {
                        n.chave.clone()
                    } else {
                        n.emitente.clone()
                    };
                    ui.add(Rotulo::interface(nome));
                });
                row.col(|ui| {
                    ui.add(Rotulo::campo(
                        n.emissao.map(|d| d.formatar()).unwrap_or_default(),
                    ));
                });
                row.col(|ui| {
                    ui.add(ValorDinheiro::novo(n.valor).neutro());
                });
                row.col(|ui| {
                    let etiqueta = match n.estado {
                        EstadoNaSefaz::Aguardando => Etiqueta::atencao(n.estado.rotulo()),
                        EstadoNaSefaz::Baixada => Etiqueta::positiva(n.estado.rotulo()),
                        EstadoNaSefaz::Ignorada => Etiqueta::neutra(n.estado.rotulo()),
                        EstadoNaSefaz::Erro => Etiqueta::negativa(n.estado.rotulo()),
                    };
                    ui.add(etiqueta);
                });
                row.col(|ui| {
                    ui.add(Rotulo::campo(n.motivo.clone().unwrap_or_default()).quebravel());
                });
                row.col(|ui| {
                    if n.estado == EstadoNaSefaz::Erro
                        && pode_importar
                        && ui
                            .add(Botao::primario("Importar de novo").pequeno())
                            .clicked()
                    {
                        de_novo = Some(n.chave.clone());
                    }
                });
            });
        });
    if let Some(chave) = de_novo {
        let r: Result<RelatorioImportacao, _> = motor.executar(
            sessao,
            "compras.importar_de_novo_da_sefaz.v1",
            &ImportarDeNovoDaSefaz { chave },
        );
        match r {
            Ok(_) => {
                notificar(ui.ctx(), Notificacao::sucesso("Nota importada"));
                estado.carregar(motor, sessao);
            }
            Err(e) => notificar(ui.ctx(), Notificacao::erro(e.mensagem)),
        }
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
    motor: &Motor,
    sessao: &Sessao,
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

fn lancar(ctx: &egui::Context, motor: &Motor, sessao: &Sessao, estado: &mut EstadoTelaCompras) {
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
    motor: &Motor,
    sessao: &Sessao,
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
                if let Some(alerta) = &n.alerta {
                    ui.add_space(Espaco::E8);
                    Painel::novo().realce(Tom::Atencao).mostrar(ui, |ui| {
                        ui.add(Rotulo::interface(alerta).quebravel());
                    });
                }

                ui.add_space(Espaco::E12);
                ui.add(Rotulo::titulo_secao("Itens e casamento"));
                ui.add_space(Espaco::E4);

                let nomes_produto: HashMap<Id, String> = estado
                    .produtos
                    .iter()
                    .map(|p| (p.produto, p.nome.clone()))
                    .collect();
                let mut acao: Option<(Id, AcaoItem)> = None;
                let grupos = estado.grupos.clone();
                for it in estado.itens_nota.clone() {
                    let opcoes = opcoes_busca_produtos(&estado.produtos);
                    if it.estado_casamento == EstadoCasamento::NaoCasado {
                        estado.form_produto_novo(motor, sessao, &it);
                    }
                    let busca = estado.busca_produto.entry(it.id).or_default();
                    let sel = estado.vinc_produto.entry(it.id).or_default();
                    let form = estado.produto_novo.get_mut(&it.id);
                    if let Some(a) = linha_item_casamento(
                        ui,
                        &it,
                        &nomes_produto,
                        opcoes,
                        busca,
                        sel,
                        form.map(|f| (f, grupos.as_slice())),
                    ) {
                        acao = Some((it.id, a));
                    }
                    ui.add_space(Espaco::E8);
                }
                match acao {
                    Some((item_nota, AcaoItem::Vincular(produto))) => {
                        vincular_item(ui.ctx(), motor, sessao, estado, n.nota, item_nota, produto);
                    }
                    Some((item_nota, AcaoItem::Criar)) => {
                        criar_produto_do_item(ui.ctx(), motor, sessao, estado, n.nota, item_nota);
                    }
                    None => {}
                }

                // A nota em conferência se confirma quando todo item tem produto (`Conferida`
                // nunca é alcançada por nenhum comando: esperar por ela escondia o botão).
                if matches!(
                    n.estado,
                    EstadoNotaEntrada::AConferir | EstadoNotaEntrada::Conferida
                ) {
                    ui.add_space(Espaco::E12);
                    ui.add(Divisor::novo());
                    ui.add_space(Espaco::E12);
                    ui.add(Rotulo::titulo_secao("Confirmar entrada"));
                    ui.add_space(Espaco::E8);
                    let pendentes = estado
                        .itens_nota
                        .iter()
                        .filter(|i| i.estado_casamento != EstadoCasamento::Casado)
                        .count();
                    if pendentes > 0 {
                        ui.add(
                            Rotulo::campo(format!(
                                "Vincule {} a um produto do estoque para dar entrada.",
                                if pendentes == 1 {
                                    "o item que falta".to_owned()
                                } else {
                                    format!("os {pendentes} itens que faltam")
                                }
                            ))
                            .quebravel(),
                        );
                    } else {
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
                                Rotulo::campo(
                                    "O título nasce quitado, numa parcela só, em vez de \
                                     pendente.",
                                )
                                .cor(ui.cores().positivo),
                            );
                        } else {
                            ui.add_space(Espaco::E8);
                            parcelas::secao(
                                ui,
                                &mut estado.parcelas,
                                n.valor_total,
                                n.data_emissao,
                            );
                        }
                        ui.add_space(Espaco::E12);
                        if ui.add(Botao::primario("Confirmar entrada")).clicked() {
                            confirmar(ui.ctx(), motor, sessao, estado, n.nota);
                        }
                    }
                }
                if matches!(
                    n.estado,
                    EstadoNotaEntrada::Confirmada | EstadoNotaEntrada::Devolvida
                ) {
                    ui.add_space(Espaco::E12);
                    ui.add(Divisor::novo());
                    ui.add_space(Espaco::E12);
                    let itens = estado.itens_nota.clone();
                    if devolucao::secao(
                        ui,
                        motor,
                        sessao,
                        &mut estado.devolucao,
                        n.nota,
                        &itens,
                        n.estado == EstadoNotaEntrada::Confirmada,
                    ) {
                        estado.carregar(motor, sessao);
                        if let Some(aberta) = estado.notas.iter().find(|x| x.nota == n.nota) {
                            estado.nota_aberta = Some(aberta.clone());
                        }
                    }
                }
                if n.estado == EstadoNotaEntrada::Confirmada {
                    ui.add_space(Espaco::E12);
                    ui.add(Divisor::novo());
                    ui.add_space(Espaco::E12);
                    if estorno::secao(ui, motor, sessao, &mut estado.estorno, n.nota) {
                        estado.carregar(motor, sessao);
                        estado.recarregar_itens(motor, sessao, n.nota);
                        if let Some(aberta) = estado.nota_aberta.clone() {
                            estado.parcelas =
                                parcelas::EditorParcelas::carregar(motor, sessao, &aberta);
                        }
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
/// Devolve a ação (vincular a um produto, ou criar o produto novo) quando alguma disparou neste
/// quadro. `produto_novo` é o formulário de criar (só para item não casado) e a árvore de grupos.
fn linha_item_casamento(
    ui: &mut egui::Ui,
    item: &ItemNotaEntrada,
    nomes_produto: &HashMap<Id, String>,
    opcoes: Vec<OpcaoBusca<Id>>,
    busca: &mut String,
    selecionado: &mut Option<Id>,
    produto_novo: Option<(&mut FormProdutoNovo, &[NoArvore])>,
) -> Option<AcaoItem> {
    let cores = ui.cores();
    let mut vincular = None;
    let mut criar = false;
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
                // De onde vem o custo: desconto, despesas rateadas e impostos que não voltam.
                let mut partes = Vec::new();
                if item.valor_desconto.e_positivo() {
                    partes.push(format!(
                        "desconto {}",
                        item.valor_desconto.formatar_com_simbolo()
                    ));
                }
                if item.valor_rateio.e_positivo() {
                    partes.push(format!(
                        "frete/despesas {}",
                        item.valor_rateio.formatar_com_simbolo()
                    ));
                }
                if item.valor_impostos.e_positivo() {
                    partes.push(format!(
                        "ICMS-ST/IPI {}",
                        item.valor_impostos.formatar_com_simbolo()
                    ));
                }
                if !partes.is_empty() {
                    ui.add(Rotulo::campo(format!(
                        "{} → custo {} por unidade",
                        partes.join(" · "),
                        item.custo_final_unitario()
                    )));
                }
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
                if let Some((form, grupos)) = produto_novo {
                    ui.add_space(Espaco::E4);
                    SecaoExpansivel::nova("Não está no catálogo? Criar produto novo").mostrar(
                        ui,
                        |ui| {
                            ui.add(Rotulo::campo(
                                "O nome vem padronizado pelo dicionário da empresa; o que você \
                                 corrigir aqui ele aprende para a próxima nota. NCM e código de \
                                 barras vêm da nota.",
                            ));
                            ui.add_space(Espaco::E4);
                            ui.add(Campo::novo("Nome do produto", &mut form.nome));
                            ui.add_space(Espaco::E4);
                            SeletorEmArvore::novo(
                                "Grupo",
                                &mut form.grupo,
                                grupos,
                                &mut form.seletor,
                            )
                            .marcador("Sem grupo")
                            .opcao_vazia("Sem grupo")
                            .mostrar(ui);
                            ui.add_space(Espaco::E4);
                            if ui
                                .add_enabled(
                                    !form.nome.trim().is_empty(),
                                    Botao::primario("Criar e vincular").pequeno(),
                                )
                                .clicked()
                            {
                                criar = true;
                            }
                        },
                    );
                }
            }
        }
    });

    if criar {
        return Some(AcaoItem::Criar);
    }
    vincular.map(AcaoItem::Vincular)
}

fn criar_produto_do_item(
    ctx: &egui::Context,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaCompras,
    nota: Id,
    item_nota: Id,
) {
    let Some(form) = estado.produto_novo.get(&item_nota) else {
        return;
    };
    let comando = CriarProdutoDoItem {
        item_nota,
        nome: form.nome.trim().to_owned(),
        grupo: form.grupo,
    };
    match motor.executar::<CriarProdutoDoItem>(sessao, "compras.criar_produto_do_item.v1", &comando)
    {
        Ok(r) => {
            let r: ProdutoDoItemCriado = r;
            estado.produto_novo.remove(&item_nota);
            estado.recarregar_itens(motor, sessao, nota);
            estado.carregar(motor, sessao);
            let extra = if r.termos_aprendidos > 0 {
                format!(
                    " — o dicionário aprendeu {} termo(s) com a sua correção",
                    r.termos_aprendidos
                )
            } else {
                String::new()
            };
            notificar(
                ctx,
                Notificacao::sucesso(format!("Produto criado e vinculado{extra}")),
            );
        }
        Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
    }
}

fn vincular_item(
    ctx: &egui::Context,
    motor: &Motor,
    sessao: &Sessao,
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
    motor: &Motor,
    sessao: &Sessao,
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
    // Pago no ato: uma parcela só, quitada — as parcelas não vão.
    let parcelas = if estado.pago_no_ato {
        None
    } else {
        match estado.parcelas.lidas() {
            Ok(p) => Some(p),
            Err(e) => {
                notificar(ctx, Notificacao::aviso(e));
                return;
            }
        }
    };
    match motor.executar(
        sessao,
        "compras.confirmar_entrada.v1",
        &ConfirmarEntrada {
            nota_entrada: nota,
            local,
            gerar_titulo_a_pagar: None,
            pago_no_ato: Some(estado.pago_no_ato),
            parcelas,
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
            // O custo mudou: avisa se algum preço de venda ficou para trás (só lembrete).
            let defasados = crate::aviso_defasados::contar_defasados(motor, sessao);
            if let Some(aviso) = crate::aviso_defasados::texto_do_aviso(defasados) {
                notificar(ctx, Notificacao::aviso(aviso));
            }
        }
        Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::testes_comum::motor_de_teste;

    const XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<nfeProc xmlns="http://www.portalfiscal.inf.br/nfe">
  <NFe>
    <infNFe Id="NFe11111111111111111111111111111111111111111111" versao="4.00">
      <ide><nNF>12</nNF><serie>1</serie><dhEmi>2026-10-01T09:00:00-03:00</dhEmi></ide>
      <emit><CNPJ>45543915000181</CNPJ><xNome>Atacado Beta LTDA</xNome></emit>
      <det nItem="1">
        <prod>
          <cProd>FER-10</cProd>
          <cEAN>7891000315507</cEAN>
          <xProd>FERM BIOLOGICO SECO FLEISCHMANN 10G</xProd>
          <NCM>21021090</NCM>
          <qCom>24.0000</qCom>
          <vUnCom>1.500000</vUnCom>
          <vProd>36.00</vProd>
        </prod>
      </det>
      <total><ICMSTot><vProd>36.00</vProd><vFrete>0.00</vFrete><vSeg>0.00</vSeg>
        <vOutro>0.00</vOutro><vNF>36.00</vNF></ICMSTot></total>
    </infNFe>
  </NFe>
</nfeProc>"#;

    #[test]
    fn item_nao_casado_cria_produto_com_o_nome_sugerido_e_corrigido() {
        let t = motor_de_teste();
        let r: mod_compras::RelatorioImportacao = t
            .motor
            .executar(
                &t.sessao,
                "compras.importar_nota_de_arquivo_xml.v1",
                &ImportarNotaDeArquivoXml { xml: XML.into() },
            )
            .expect("importa");
        let mut estado = EstadoTelaCompras::default();
        estado.carregar(&t.motor, &t.sessao);
        estado.abrir_nota(&t.motor, &t.sessao, r.nota_entrada);
        assert_eq!(estado.itens_nota.len(), 1);
        let item = estado.itens_nota[0].clone();
        assert_eq!(item.estado_casamento, EstadoCasamento::NaoCasado);

        // O diálogo desenha o item com o formulário "Criar produto novo" já sugerido.
        let ctx = egui::Context::default();
        cardeal_ui::tokens::instalar_fontes(&ctx);
        cardeal_ui::tokens::instalar_estilo(&ctx, cardeal_ui::tokens::Tema::Claro);
        for _ in 0..2 {
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                dialogo_ver(ctx, &t.motor, &t.sessao, &mut estado);
            });
        }
        let form = estado.produto_novo.get_mut(&item.id).expect("formulário");
        assert_eq!(form.nome, "Ferm Biologico Seco Fleischmann 10gr");
        form.nome = "Fermento Biológico Seco Fleischmann 10gr".into();

        criar_produto_do_item(
            &ctx,
            &t.motor,
            &t.sessao,
            &mut estado,
            r.nota_entrada,
            item.id,
        );
        assert_eq!(
            estado.itens_nota[0].estado_casamento,
            EstadoCasamento::Casado
        );
        assert!(estado
            .produtos
            .iter()
            .any(|p| p.nome == "Fermento Biológico Seco Fleischmann 10gr"
                && p.codigo_barras.as_deref() == Some("7891000315507")));
        assert!(
            estado.produto_novo.is_empty(),
            "o formulário sai depois de criar"
        );
    }

    #[test]
    fn nota_vinculada_confirma_em_parcelas_e_se_estorna_pela_tela() {
        let t = motor_de_teste();
        let r: mod_compras::RelatorioImportacao = t
            .motor
            .executar(
                &t.sessao,
                "compras.importar_nota_de_arquivo_xml.v1",
                &ImportarNotaDeArquivoXml { xml: XML.into() },
            )
            .expect("importa");
        let mut estado = EstadoTelaCompras::default();
        estado.carregar(&t.motor, &t.sessao);
        estado.abrir_nota(&t.motor, &t.sessao, r.nota_entrada);
        let item = estado.itens_nota[0].clone();
        let ctx = egui::Context::default();
        cardeal_ui::tokens::instalar_fontes(&ctx);
        cardeal_ui::tokens::instalar_estilo(&ctx, cardeal_ui::tokens::Tema::Claro);
        let quadro = |estado: &mut EstadoTelaCompras| {
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                dialogo_ver(ctx, &t.motor, &t.sessao, estado);
            });
        };
        quadro(&mut estado);
        // Sem produto vinculado, o diálogo pede o vínculo em vez de oferecer a confirmação.
        criar_produto_do_item(
            &ctx,
            &t.motor,
            &t.sessao,
            &mut estado,
            r.nota_entrada,
            item.id,
        );
        assert_eq!(
            estado.itens_nota[0].estado_casamento,
            EstadoCasamento::Casado
        );
        // A nota está "A conferir" com tudo vinculado: o diálogo oferece a confirmação.
        quadro(&mut estado);
        assert_eq!(
            estado.parcelas.lidas().unwrap().len(),
            1,
            "sem duplicatas, uma parcela na emissão"
        );
        estado.parcelas = parcelas::EditorParcelas::carregar(
            &t.motor,
            &t.sessao,
            estado.nota_aberta.as_ref().unwrap(),
        );
        let local: mod_estoque::LocalCriado = t
            .motor
            .executar(
                &t.sessao,
                "estoque.criar_local.v1",
                &mod_estoque::CriarLocal {
                    nome: "Depósito".into(),
                    tipo: mod_estoque::TipoLocal::Deposito,
                },
            )
            .unwrap();
        estado.local_sel = Some(local.local);
        confirmar(&ctx, &t.motor, &t.sessao, &mut estado, r.nota_entrada);
        estado.carregar(&t.motor, &t.sessao);
        let nota = estado
            .notas
            .iter()
            .find(|n| n.nota == r.nota_entrada)
            .unwrap();
        assert_eq!(nota.estado, EstadoNotaEntrada::Confirmada);

        // Estornar: a prévia, o motivo e a nota de volta à conferência.
        estado.abrir_nota(&t.motor, &t.sessao, r.nota_entrada);
        quadro(&mut estado);
        let feito: mod_compras::EntradaFoiEstornada = t
            .motor
            .executar(
                &t.sessao,
                "compras.estornar_entrada.v1",
                &mod_compras::EstornarEntrada {
                    nota_entrada: r.nota_entrada,
                    motivo: "Quantidade errada na conferência".into(),
                    estornar_pagamento: false,
                },
            )
            .expect("estorna");
        assert_eq!(feito.itens, 1);
        estado.carregar(&t.motor, &t.sessao);
        assert_eq!(
            estado.nota_aberta.as_ref().map(|n| n.estado),
            Some(EstadoNotaEntrada::AConferir)
        );
        quadro(&mut estado);
        quadro(&mut estado);
    }

    #[test]
    fn nota_confirmada_volta_ao_fornecedor_pela_tela() {
        let t = motor_de_teste();
        let r: mod_compras::RelatorioImportacao = t
            .motor
            .executar(
                &t.sessao,
                "compras.importar_nota_de_arquivo_xml.v1",
                &ImportarNotaDeArquivoXml { xml: XML.into() },
            )
            .expect("importa");
        let mut estado = EstadoTelaCompras::default();
        estado.carregar(&t.motor, &t.sessao);
        estado.abrir_nota(&t.motor, &t.sessao, r.nota_entrada);
        let item = estado.itens_nota[0].clone();
        let ctx = egui::Context::default();
        cardeal_ui::tokens::instalar_fontes(&ctx);
        cardeal_ui::tokens::instalar_estilo(&ctx, cardeal_ui::tokens::Tema::Claro);
        let quadro = |estado: &mut EstadoTelaCompras| {
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                dialogo_ver(ctx, &t.motor, &t.sessao, estado);
            });
        };
        quadro(&mut estado);
        criar_produto_do_item(
            &ctx,
            &t.motor,
            &t.sessao,
            &mut estado,
            r.nota_entrada,
            item.id,
        );
        quadro(&mut estado);
        estado.parcelas = parcelas::EditorParcelas::carregar(
            &t.motor,
            &t.sessao,
            estado.nota_aberta.as_ref().unwrap(),
        );
        let local: mod_estoque::LocalCriado = t
            .motor
            .executar(
                &t.sessao,
                "estoque.criar_local.v1",
                &mod_estoque::CriarLocal {
                    nome: "Depósito".into(),
                    tipo: mod_estoque::TipoLocal::Deposito,
                },
            )
            .unwrap();
        estado.local_sel = Some(local.local);
        confirmar(&ctx, &t.motor, &t.sessao, &mut estado, r.nota_entrada);
        estado.carregar(&t.motor, &t.sessao);
        estado.abrir_nota(&t.motor, &t.sessao, r.nota_entrada);
        quadro(&mut estado);
        quadro(&mut estado);

        // Tudo volta: a nota fica "Devolvida", e a devolução aparece com a NF-e a emitir.
        let feito: mod_compras::DevolucaoFeita = t
            .motor
            .executar(
                &t.sessao,
                "compras.devolver_ao_fornecedor.v1",
                &mod_compras::DevolverAoFornecedor {
                    nota_entrada: r.nota_entrada,
                    itens: vec![mod_compras::ItemADevolver {
                        item: item.id,
                        quantidade: item.quantidade,
                    }],
                    motivo: "Produto com a validade vencida".into(),
                },
            )
            .expect("devolve");
        assert!(feito.nota_devolvida);
        assert_eq!(feito.abatido, feito.valor_total, "a conta a pagar abatida");
        estado.carregar(&t.motor, &t.sessao);
        estado.abrir_nota(&t.motor, &t.sessao, r.nota_entrada);
        assert_eq!(
            estado.nota_aberta.as_ref().map(|n| n.estado),
            Some(EstadoNotaEntrada::Devolvida)
        );
        quadro(&mut estado);
        quadro(&mut estado);
        // Sem a empresa pronta para emitir, a NF-e de devolução nasce rejeitada, com o porquê.
        let doc: mod_fiscal::DocumentoResumo = t
            .motor
            .executar(
                &t.sessao,
                "compras.emitir_nfe_da_devolucao.v1",
                &mod_compras::EmitirNfeDaDevolucao {
                    devolucao: feito.devolucao,
                },
            )
            .expect("cria a NF-e");
        assert_eq!(doc.estado, mod_fiscal::EstadoDocumento::Rejeitada);
        assert!(doc.motivo.is_some());
        estado.abrir_nota(&t.motor, &t.sessao, r.nota_entrada);
        quadro(&mut estado);
        quadro(&mut estado);
    }
}
