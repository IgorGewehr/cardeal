//! Tela de Estoque — lista de produtos + radar de reposição + dialog (criar / ver / repor).
//! `docs/modulos/estoque.md`.
//!
//! Padrão de UI do projeto: a tela abre na lista inteira; "+ Novo produto" e o clique numa
//! linha abrem o mesmo `Dialogo`. Duas abas: "Produtos" (o catálogo) e "Radar de reposição"
//! (produtos abaixo do ponto de pedido — `estoque.produtos_abaixo_do_ponto_pedido.v1`, já
//! existia no backend mas não estava ligado a nenhuma tela).

use cardeal_cliente::{MotorLocal, SessaoLocal};
use cardeal_kernel::{Fuso, Id, Preco, Quantidade};
use cardeal_modkit::Icone;
use cardeal_ui::atoms::{Botao, Etiqueta, Rotulo, Tom, ATALHO_NOVO};
use cardeal_ui::molecules::{Abas, Campo, CartaoKpi, EstadoVazio, SecaoExpansivel, SeletorOpcao};
use cardeal_ui::organisms::{
    notificar, ColunaGrade, Dialogo, Direcao, FaixaKpi, Grade, LayoutTela, Notificacao,
};
use cardeal_ui::tokens::{Espaco, TemaUi};
use eframe::egui;
use mod_estoque::{
    CriarGrupoProduto, CriarLocal, CriarProduto, CriarUnidade, DefinirPontoPedido,
    DetalhesTecnicos, EntradaRegistrada, GrupoProdutoCriado, GruposProduto,
    ItemAbaixoDoPontoPedido, ItemGrupoProduto, ItemLocal, ItemProdutoComSaldo, ItemUnidade, Locais,
    LocalCriado, Movimento, MovimentosDoProduto, ProdutoCriado, ProdutosAbaixoDoPontoPedido,
    ProdutosComSaldo, RegistrarEntrada, TipoLocal, TipoMovimento, UnidadeCriada, Unidades,
};

/// Qual aba da tela de Estoque está ativa.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum AbaEstoque {
    #[default]
    Produtos,
    Radar,
}

#[derive(Default, PartialEq)]
enum Dlg {
    #[default]
    Fechado,
    Novo,
    Ver(usize),
    /// Editar os detalhes técnicos de um produto já cadastrado (índice em `estado.produtos`)
    /// — nome/NCM/código de barras continuam sem edição nesta fatia (`EditarDetalhesTecnicosProduto`
    /// só cobre os campos técnicos, ver `docs/modulos/estoque.md` §5).
    Editar(usize),
    /// Repor estoque de um item do radar (índice em `estado.radar`).
    Repor(usize),
}

/// O estado local da tela.
#[derive(Default)]
pub struct EstadoTelaEstoque {
    aba: AbaEstoque,
    produtos: Vec<ItemProdutoComSaldo>,
    grupos: Vec<ItemGrupoProduto>,
    unidades: Vec<ItemUnidade>,
    locais: Vec<ItemLocal>,
    radar: Vec<ItemAbaixoDoPontoPedido>,
    /// Movimentações recentes do produto aberto no dialog "Ver" — a rastreabilidade
    /// peça↔origem já existente no backend (`estoque.movimentos_do_produto.v1`).
    movimentos: Vec<Movimento>,
    busca: String,
    erro: Option<String>,
    dlg: Dlg,
    ordenacao: Option<(usize, Direcao)>,
    /// `(produto, nome)` aguardando confirmação de exclusão (desativação).
    confirmar_exclusao: Option<(Id, String)>,

    novo_grupo_codigo: String,
    novo_grupo_nome: String,
    nova_unidade_sigla: String,
    nova_unidade_nome: String,
    novo_local_nome: String,

    grupo_selecionado: Option<Id>,
    unidade_selecionada: Option<Id>,
    produto_nome: String,
    produto_ncm: String,
    produto_codigo_barras: String,
    local_selecionado: Option<Id>,
    estoque_inicial_qtd: String,
    estoque_inicial_custo: String,

    // Detalhes técnicos (opcional) — cadastro de peça de microeletrônica.
    det_fabricante: String,
    det_codigo_fabricante: String,
    det_categoria_tecnica: String,
    det_especificacao_tecnica: String,
    det_compatibilidade: String,
    det_garantia_fornecedor_dias: String,
    det_localizacao_fisica: String,

    // Reposição (radar → `Dlg::Repor`).
    rep_local: Option<Id>,
    rep_qtd: String,
    rep_custo: String,
    rep_ponto: String,
    rep_minimo: String,
}

impl EstadoTelaEstoque {
    /// Recarrega produtos, grupos, unidades, locais e o radar de reposição.
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
        if let Ok(r) = motor.consultar(
            sessao,
            "estoque.produtos_abaixo_do_ponto_pedido.v1",
            &ProdutosAbaixoDoPontoPedido,
        ) {
            self.radar = r;
        }
    }

    fn limpar_novo(&mut self) {
        self.produto_nome.clear();
        self.produto_ncm.clear();
        self.produto_codigo_barras.clear();
        self.estoque_inicial_qtd.clear();
        self.estoque_inicial_custo.clear();
        self.det_fabricante.clear();
        self.det_codigo_fabricante.clear();
        self.det_categoria_tecnica.clear();
        self.det_especificacao_tecnica.clear();
        self.det_compatibilidade.clear();
        self.det_garantia_fornecedor_dias.clear();
        self.det_localizacao_fisica.clear();
    }

    /// Filtra `produtos` pelo termo de busca (nome ou NCM) — client-side: a consulta não
    /// aceita termo de busca (traz até 500 linhas de uma vez), e o catálogo típico de uma
    /// assistência cabe folgado nisso.
    ///
    /// `TODO(backend)`: quando a rastreabilidade por código curto (post-it) ganhar um campo
    /// em `ItemProdutoComSaldo`/`estoque_produto`, somar `p.codigo_curto` aqui é o suficiente
    /// para a busca já cobrir o código — nenhuma outra mudança de UI necessária.
    fn produtos_filtrados(&self) -> Vec<usize> {
        let termo = self.busca.trim().to_lowercase();
        (0..self.produtos.len())
            .filter(|&i| {
                termo.is_empty() || {
                    let p = &self.produtos[i];
                    p.nome.to_lowercase().contains(&termo) || p.ncm.contains(&termo)
                }
            })
            .collect()
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
            if ui
                .add(Botao::primario("+ Novo produto").tecla(ATALHO_NOVO))
                .clicked()
            {
                estado.limpar_novo();
                estado.dlg = Dlg::Novo;
            }
        },
        |ui, estado| {
            if let Some(nova) = Abas::nova(&[
                (AbaEstoque::Produtos, "Produtos"),
                (AbaEstoque::Radar, "Radar de reposição"),
            ])
            .selecionada(estado.aba)
            .mostrar(ui)
            {
                estado.aba = nova;
            }
            ui.add_space(Espaco::E16);

            if let Some(erro) = &estado.erro {
                ui.add(
                    Rotulo::interface(erro.clone())
                        .quebravel()
                        .cor(ui.cores().negativo),
                );
                ui.add_space(Espaco::E12);
            }

            match estado.aba {
                AbaEstoque::Produtos => lista(ui, motor, sessao, estado),
                AbaEstoque::Radar => radar(ui, estado),
            }
        },
    );

    match estado.dlg {
        Dlg::Fechado => {}
        Dlg::Novo => dialogo_novo(ui.ctx(), motor, sessao, estado),
        Dlg::Ver(i) => dialogo_ver(ui.ctx(), motor, sessao, estado, i),
        Dlg::Editar(i) => dialogo_editar(ui.ctx(), motor, sessao, estado, i),
        Dlg::Repor(i) => dialogo_repor(ui.ctx(), motor, sessao, estado, i),
    }

    if let Some((produto, nome)) = estado.confirmar_exclusao.clone() {
        let resposta = cardeal_ui::organisms::dialogo_confirmacao(
            ui.ctx(),
            "Excluir produto",
            &format!(
                "Tem certeza que quer excluir \"{nome}\"? Ele sai da busca padrão, mas o \
                 saldo, os lotes e o histórico continuam intactos — dá pra reativar depois.",
            ),
            "Excluir",
        );
        if resposta.confirmado {
            match motor.executar(
                sessao,
                "estoque.definir_ativo_produto.v1",
                &mod_estoque::DefinirAtivoProduto {
                    produto,
                    ativo: false,
                },
            ) {
                Ok(()) => {
                    estado.dlg = Dlg::Fechado;
                    estado.carregar(motor, sessao);
                    notificar(ui.ctx(), Notificacao::sucesso("Produto excluído"));
                }
                Err(e) => notificar(ui.ctx(), Notificacao::erro(e.mensagem)),
            }
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
    estado: &mut EstadoTelaEstoque,
) {
    if estado.produtos.is_empty() {
        if estado.erro.is_none()
            && EstadoVazio::novo(Icone::Estoque, "Nenhum produto cadastrado ainda.")
                .acao("Cadastrar o primeiro")
                .mostrar(ui)
        {
            estado.limpar_novo();
            estado.dlg = Dlg::Novo;
        }
        return;
    }

    let sem_estoque = estado
        .produtos
        .iter()
        .filter(|p| p.disponivel.e_zero())
        .count();
    FaixaKpi::nova(vec![
        CartaoKpi::contagem("Produtos cadastrados", estado.produtos.len()),
        CartaoKpi::contagem("Sem estoque disponível", sem_estoque),
        CartaoKpi::contagem("Abaixo do ponto de pedido", estado.radar.len()).variacao(
            if estado.radar.is_empty() {
                "radar limpo"
            } else {
                "ver aba Radar de reposição"
            },
        ),
    ])
    .mostrar(ui);

    ui.horizontal(|ui| {
        ui.set_max_width(360.0);
        // TODO(backend): quando o código curto de rastreabilidade existir, o marcador vira
        // "Buscar por nome, NCM ou código" e `produtos_filtrados` passa a comparar contra
        // ele também.
        ui.add(Campo::novo("", &mut estado.busca).marcador("Buscar por nome ou NCM"));
    });
    ui.add_space(Espaco::E12);

    let indices = estado.produtos_filtrados();
    if indices.is_empty() {
        ui.add(Rotulo::interface("Nenhum produto para essa busca.").cor(ui.cores().texto_medio));
        return;
    }

    let colunas = vec![
        ColunaGrade::nova("Produto"),
        ColunaGrade::nova("NCM").largura(110.0),
        ColunaGrade::nova("Disponível").largura(130.0).numero(),
        ColunaGrade::nova("Reservado").largura(110.0).numero(),
        ColunaGrade::nova("Custo médio").largura(120.0).numero(),
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
        .mostrar(ui, indices.len(), |i, row| {
            let p = &estado.produtos[indices[i]];
            row.col(|ui| {
                ui.add(Rotulo::interface(p.nome.clone()));
            });
            row.col(|ui| {
                ui.add(Rotulo::campo(p.ncm.clone()));
            });
            row.col(|ui| {
                ui.horizontal(|ui| {
                    ui.add(Rotulo::interface(p.disponivel.to_string()));
                    if p.disponivel.e_zero() {
                        ui.add(Etiqueta::negativa("sem estoque"));
                    }
                });
            });
            row.col(|ui| {
                ui.add(Rotulo::interface(p.reservado.to_string()));
            });
            row.col(|ui| {
                ui.add(Rotulo::interface(p.custo_medio.to_string()));
            });
            row.col(|ui| {
                let rubro = ui.cores().rubro;
                ui.horizontal(|ui| {
                    if ui.add(Botao::fantasma("Editar").pequeno().cor(rubro)).clicked() {
                        editar_clicado = Some(indices[i]);
                    }
                    if ui.add(Botao::destrutivo("Excluir").pequeno()).clicked() {
                        excluir_clicado = Some((p.produto, p.nome.clone()));
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
        ordenar_produtos(&mut estado.produtos, coluna, direcao);
    }
    if let Some(indice) = editar_clicado {
        abrir_editar(motor, sessao, estado, indice);
    } else if let Some(produto) = excluir_clicado {
        estado.confirmar_exclusao = Some(produto);
    } else if let Some(i) = resposta.linha_clicada {
        estado.dlg = Dlg::Ver(indices[i]);
    }
}

/// Carrega os detalhes técnicos atuais do produto (`ProdutoPorId`) nos campos do formulário
/// e abre o dialog de edição — precisa vir do backend, não de `ItemProdutoComSaldo` (que não
/// carrega esses campos), senão "Salvar" apagaria o que já estava cadastrado
/// (`EditarDetalhesTecnicosProduto` substitui os detalhes técnicos por completo).
fn abrir_editar(
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaEstoque,
    indice: usize,
) {
    let produto = estado.produtos[indice].produto;
    match motor.consultar(
        sessao,
        "estoque.produto_por_id.v1",
        &mod_estoque::ProdutoPorId { produto },
    ) {
        Ok(Some(p)) => {
            let p: mod_estoque::Produto = p;
            estado.det_fabricante = p.fabricante.unwrap_or_default();
            estado.det_codigo_fabricante = p.codigo_fabricante.unwrap_or_default();
            estado.det_categoria_tecnica = p.categoria_tecnica.unwrap_or_default();
            estado.det_especificacao_tecnica = p.especificacao_tecnica.unwrap_or_default();
            estado.det_compatibilidade = p.compatibilidade.unwrap_or_default();
            estado.det_garantia_fornecedor_dias = p
                .garantia_fornecedor_dias
                .map_or_else(String::new, |d| d.to_string());
            estado.det_localizacao_fisica = p.localizacao_fisica.unwrap_or_default();
            estado.dlg = Dlg::Editar(indice);
        }
        Ok(None) => estado.erro = Some("Produto não encontrado.".to_owned()),
        Err(e) => estado.erro = Some(e.mensagem),
    }
}

/// A aba "Radar de reposição": produtos cujo disponível caiu abaixo do ponto de pedido
/// cadastrado (`ItemAbaixoDoPontoPedido`). Clicar numa linha abre "Repor estoque", que já
/// mostra os números atuais e deixa lançar a entrada sem sair do fluxo.
fn radar(ui: &mut egui::Ui, estado: &mut EstadoTelaEstoque) {
    if estado.radar.is_empty() {
        EstadoVazio::novo(
            Icone::Alerta,
            "Nada abaixo do ponto de pedido — estoque saudável.",
        )
        .mostrar(ui);
        return;
    }

    let colunas = vec![
        ColunaGrade::nova("Produto"),
        ColunaGrade::nova("Disponível").largura(110.0).numero(),
        ColunaGrade::nova("Ponto de pedido").largura(130.0).numero(),
        ColunaGrade::nova("Estoque mínimo").largura(130.0).numero(),
        ColunaGrade::nova("Situação").largura(140.0),
    ];
    let resposta =
        Grade::nova(colunas)
            .selecionavel(None)
            .mostrar(ui, estado.radar.len(), |i, row| {
                let item = &estado.radar[i];
                row.col(|ui| {
                    ui.add(Rotulo::interface(item.nome.clone()));
                });
                row.col(|ui| {
                    ui.add(Rotulo::interface(item.disponivel.to_string()));
                });
                row.col(|ui| {
                    ui.add(Rotulo::interface(item.ponto_pedido.to_string()));
                });
                row.col(|ui| {
                    ui.add(Rotulo::interface(
                        item.estoque_minimo
                            .map_or_else(|| "—".to_owned(), |m| m.to_string()),
                    ));
                });
                row.col(|ui| {
                    if item.disponivel.e_zero() {
                        ui.add(Etiqueta::negativa("Sem estoque"));
                    } else {
                        ui.add(Etiqueta::atencao("Abaixo do ponto"));
                    }
                });
            });
    if let Some(i) = resposta.linha_clicada {
        let item = &estado.radar[i];
        estado.rep_local = estado.locais.first().map(|l| l.id);
        estado.rep_qtd.clear();
        estado.rep_custo.clear();
        estado.rep_ponto = item.ponto_pedido.to_string();
        estado.rep_minimo = item
            .estoque_minimo
            .map_or_else(String::new, |m| m.to_string());
        estado.dlg = Dlg::Repor(i);
    }
}

/// Ordena `produtos` pela coluna clicada no cabeçalho da [`Grade`] (mesma ordem das colunas
/// declaradas em `lista`: Produto, NCM, Disponível, Reservado, Custo médio).
fn ordenar_produtos(produtos: &mut [ItemProdutoComSaldo], coluna: usize, direcao: Direcao) {
    produtos.sort_by(|a, b| {
        let ordem = match coluna {
            0 => a.nome.cmp(&b.nome),
            1 => a.ncm.cmp(&b.ncm),
            2 => a.disponivel.cmp(&b.disponivel),
            3 => a.reservado.cmp(&b.reservado),
            4 => a.custo_medio.cmp(&b.custo_medio),
            _ => std::cmp::Ordering::Equal,
        };
        match direcao {
            Direcao::Ascendente => ordem,
            Direcao::Descendente => ordem.reverse(),
        }
    });
}

/// Rótulo em português + tom da etiqueta para um [`TipoMovimento`] — o mesmo papel que
/// `situacao_amigavel` cumpre para `EstadoOs` em `tela_os.rs`.
const fn rotulo_movimento(tipo: TipoMovimento) -> (&'static str, Tom) {
    match tipo {
        TipoMovimento::Entrada => ("Entrada", Tom::Positivo),
        TipoMovimento::Saida => ("Saída", Tom::Info),
        TipoMovimento::TransferenciaSaida => ("Transferência (saída)", Tom::Info),
        TipoMovimento::TransferenciaEntrada => ("Transferência (entrada)", Tom::Info),
        TipoMovimento::AjustePositivo => ("Ajuste (+)", Tom::Positivo),
        TipoMovimento::AjusteNegativo => ("Ajuste (-)", Tom::Negativo),
        TipoMovimento::Reserva => ("Reserva", Tom::Neutro),
        TipoMovimento::LiberacaoReserva => ("Liberação de reserva", Tom::Neutro),
        TipoMovimento::Producao => ("Produção", Tom::Positivo),
        TipoMovimento::Perda => ("Perda", Tom::Negativo),
    }
}

fn dialogo_ver(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaEstoque,
    i: usize,
) {
    let Some(p) = estado.produtos.get(i).cloned() else {
        estado.dlg = Dlg::Fechado;
        return;
    };
    // Rastreabilidade: carrega uma vez por abertura do dialog, não a cada frame.
    if estado.movimentos.is_empty() {
        estado.movimentos = motor
            .consultar(
                sessao,
                "estoque.movimentos_do_produto.v1",
                &MovimentosDoProduto {
                    produto: p.produto,
                    origem_modulo: None,
                },
            )
            .unwrap_or_default();
    }

    let fechar = Dialogo::nova(p.nome.clone()).largura(620.0).mostrar(
        ctx,
        estado,
        |ui, estado| {
            ui.columns(2, |c| {
                campo_ver(&mut c[0], "NCM", &p.ncm);
                campo_ver(&mut c[1], "Custo médio", &p.custo_medio.to_string());
            });
            ui.columns(2, |c| {
                campo_ver(&mut c[0], "Disponível", &p.disponivel.to_string());
                campo_ver(&mut c[1], "Reservado", &p.reservado.to_string());
            });

            // TODO(backend): não existe hoje uma consulta "produto por id" que devolva
            // `Produto` completo (fabricante, MPN, categoria/especificação técnica,
            // compatibilidade, localização física) — só `ProdutoPorCodigoBarras`, que exige
            // saber o GTIN de antemão. Cadastro técnico continua um caminho só de ida (só
            // aparece aqui de novo quando essa consulta existir); até lá não arriscamos
            // mostrar/editar campos que não temos como ler de volta.
            ui.add_space(Espaco::E16);
            ui.separator();
            ui.add_space(Espaco::E12);
            ui.add(Rotulo::titulo_secao("Movimentações recentes"));
            ui.add_space(Espaco::E8);
            if estado.movimentos.is_empty() {
                ui.add(Rotulo::campo("Nenhuma movimentação registrada ainda."));
            } else {
                for mov in estado.movimentos.iter().take(8) {
                    let (rotulo, tom) = rotulo_movimento(mov.tipo);
                    ui.horizontal(|ui| {
                        ui.add(Etiqueta::nova(rotulo, tom));
                        ui.add(Rotulo::interface(mov.quantidade.to_string()));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.add(Rotulo::campo(mov.criado_em.formatar(Fuso::BRASILIA)));
                        });
                    });
                }
                if estado.movimentos.len() > 8 {
                    ui.add(Rotulo::campo(format!(
                        "+ {} movimentações mais antigas",
                        estado.movimentos.len() - 8
                    )));
                }
            }
        },
        |ui, estado| {
            if ui.add(Botao::primario("Editar")).clicked() {
                abrir_editar(motor, sessao, estado, i);
                estado.movimentos.clear();
            }
            if ui.add(Botao::secundario("Fechar")).clicked() {
                estado.dlg = Dlg::Fechado;
                estado.movimentos.clear();
            }
        },
    );
    if fechar {
        estado.dlg = Dlg::Fechado;
        estado.movimentos.clear();
    }
}

/// "Editar detalhes técnicos" — nome/NCM/código de barras ficam só de contexto (sem comando
/// de edição nesta fatia, `docs/modulos/estoque.md` §5); os campos técnicos reusam o mesmo
/// bloco do cadastro (`bloco_detalhes_tecnicos`).
fn dialogo_editar(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaEstoque,
    i: usize,
) {
    let Some(p) = estado.produtos.get(i).cloned() else {
        estado.dlg = Dlg::Fechado;
        return;
    };
    let enter =
        ctx.input(|i| i.key_pressed(egui::Key::Enter)) && !ctx.memory(|m| m.any_popup_open());

    let fechar = Dialogo::nova(format!("Editar — {}", p.nome))
        .largura(680.0)
        .mostrar(
            ctx,
            estado,
            |ui, estado| {
                ui.columns(2, |c| {
                    campo_ver(&mut c[0], "Produto", &p.nome);
                    campo_ver(&mut c[1], "NCM", &p.ncm);
                });
                ui.add_space(Espaco::E8);
                bloco_detalhes_tecnicos(ui, estado);
            },
            |ui, estado| {
                let clicou = ui.add(Botao::primario("Salvar")).clicked();
                if clicou || enter {
                    salvar_detalhes_tecnicos(ui.ctx(), motor, sessao, estado, p.produto);
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

fn salvar_detalhes_tecnicos(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaEstoque,
    produto: Id,
) {
    let detalhes = montar_detalhes_tecnicos(estado).unwrap_or_default();
    match motor.executar(
        sessao,
        "estoque.editar_detalhes_tecnicos_produto.v1",
        &mod_estoque::EditarDetalhesTecnicosProduto { produto, detalhes },
    ) {
        Ok(()) => {
            estado.dlg = Dlg::Fechado;
            estado.carregar(motor, sessao);
            notificar(ctx, Notificacao::sucesso("Detalhes técnicos atualizados"));
        }
        Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
    }
}

fn campo_ver(ui: &mut egui::Ui, chave: &str, valor: &str) {
    ui.add(Rotulo::campo(chave));
    ui.add(Rotulo::interface(if valor.trim().is_empty() {
        "—"
    } else {
        valor
    }));
    ui.add_space(Espaco::E12);
}

/// O dialog "Repor estoque" aberto a partir de uma linha do radar — mostra os números atuais
/// (já conhecidos, vieram do próprio radar) e permite lançar uma entrada e/ou ajustar o
/// ponto de pedido sem sair do fluxo.
fn dialogo_repor(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaEstoque,
    i: usize,
) {
    let Some(item) = estado.radar.get(i).cloned() else {
        estado.dlg = Dlg::Fechado;
        return;
    };

    let fechar = Dialogo::nova(format!("Repor estoque — {}", item.nome))
        .largura(560.0)
        .mostrar(
            ctx,
            estado,
            |ui, estado| {
                ui.columns(3, |c| {
                    campo_ver(&mut c[0], "Disponível", &item.disponivel.to_string());
                    campo_ver(&mut c[1], "Ponto de pedido", &item.ponto_pedido.to_string());
                    campo_ver(
                        &mut c[2],
                        "Estoque mínimo",
                        &item
                            .estoque_minimo
                            .map_or_else(String::new, |m| m.to_string()),
                    );
                });

                ui.add_space(Espaco::E8);
                ui.separator();
                ui.add_space(Espaco::E12);
                ui.add(Rotulo::titulo_secao("Registrar entrada"));
                ui.add_space(Espaco::E8);
                if estado.locais.is_empty() {
                    ui.add(Rotulo::interface(
                        "Nenhum local cadastrado ainda — cadastre um em \"+ Novo produto\".",
                    ));
                } else {
                    let ops: Vec<(Id, String)> = estado
                        .locais
                        .iter()
                        .map(|l| (l.id, l.nome.clone()))
                        .collect();
                    SeletorOpcao::novo("Local", &mut estado.rep_local)
                        .opcoes(ops)
                        .mostrar(ui);
                    ui.add_space(Espaco::E8);
                    ui.columns(2, |c| {
                        c[0].add(Campo::novo("Quantidade", &mut estado.rep_qtd).marcador("10"));
                        c[1].add(
                            Campo::novo("Custo unitário", &mut estado.rep_custo).marcador("90,00"),
                        );
                    });
                }

                ui.add_space(Espaco::E16);
                ui.separator();
                ui.add_space(Espaco::E12);
                ui.add(Rotulo::titulo_secao("Ponto de pedido"));
                ui.add_space(Espaco::E8);
                ui.columns(2, |c| {
                    c[0].add(Campo::novo("Ponto de pedido", &mut estado.rep_ponto));
                    c[1].add(Campo::novo(
                        "Estoque mínimo (opcional)",
                        &mut estado.rep_minimo,
                    ));
                });
            },
            |ui, estado| {
                if ui.add(Botao::secundario("Fechar")).clicked() {
                    estado.dlg = Dlg::Fechado;
                }
                if ui.add(Botao::primario("Salvar ponto de pedido")).clicked() {
                    salvar_ponto_pedido(ui.ctx(), motor, sessao, estado, item.produto);
                }
                if !estado.locais.is_empty()
                    && ui.add(Botao::primario("Registrar entrada")).clicked()
                {
                    registrar_entrada_radar(ui.ctx(), motor, sessao, estado, item.produto);
                }
            },
        );
    if fechar {
        estado.dlg = Dlg::Fechado;
    }
}

fn registrar_entrada_radar(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaEstoque,
    produto: Id,
) {
    let Some(local) = estado.rep_local else {
        notificar(ctx, Notificacao::aviso("Escolha o local que recebe."));
        return;
    };
    let (Ok(quantidade), Ok(custo_unitario)) = (
        estado.rep_qtd.parse::<Quantidade>(),
        estado.rep_custo.parse::<Preco>(),
    ) else {
        notificar(ctx, Notificacao::aviso("Quantidade ou custo inválidos."));
        return;
    };
    match motor.executar(
        sessao,
        "estoque.registrar_entrada.v1",
        &RegistrarEntrada {
            produto,
            local,
            quantidade,
            custo_unitario,
        },
    ) {
        Ok(r) => {
            let _: EntradaRegistrada = r;
            estado.dlg = Dlg::Fechado;
            estado.carregar(motor, sessao);
            notificar(ctx, Notificacao::sucesso("Entrada registrada"));
        }
        Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
    }
}

fn salvar_ponto_pedido(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaEstoque,
    produto: Id,
) {
    let ponto_pedido = estado.rep_ponto.trim().parse::<Quantidade>().ok();
    if !estado.rep_ponto.trim().is_empty() && ponto_pedido.is_none() {
        notificar(ctx, Notificacao::aviso("Ponto de pedido inválido."));
        return;
    }
    let estoque_minimo = estado.rep_minimo.trim().parse::<Quantidade>().ok();
    if !estado.rep_minimo.trim().is_empty() && estoque_minimo.is_none() {
        notificar(ctx, Notificacao::aviso("Estoque mínimo inválido."));
        return;
    }
    match motor.executar(
        sessao,
        "estoque.definir_ponto_pedido.v1",
        &DefinirPontoPedido {
            produto,
            ponto_pedido,
            estoque_minimo,
        },
    ) {
        Ok(()) => {
            estado.dlg = Dlg::Fechado;
            estado.carregar(motor, sessao);
            notificar(ctx, Notificacao::sucesso("Ponto de pedido atualizado"));
        }
        Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
    }
}

fn dialogo_novo(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaEstoque,
) {
    let pronto = estado.grupo_selecionado.is_some() && estado.unidade_selecionada.is_some();
    let enter =
        ctx.input(|i| i.key_pressed(egui::Key::Enter)) && !ctx.memory(|m| m.any_popup_open());

    let fechar = Dialogo::nova("Novo produto").largura(680.0).mostrar(
        ctx,
        estado,
        |ui, estado| corpo_novo(ui, motor, sessao, estado),
        |ui, estado| {
            let clicou = ui
                .add(Botao::primario("Cadastrar produto").habilitado(pronto))
                .clicked();
            if (clicou || enter) && pronto {
                cadastrar(ui.ctx(), motor, sessao, estado);
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
    ui.add_space(Espaco::E8);
    ui.add(
        Campo::novo(
            "Código de barras (opcional)",
            &mut estado.produto_codigo_barras,
        )
        .marcador("GTIN / EAN"),
    );

    // Estoque inicial e detalhes técnicos são os dois blocos opcionais/secundários do
    // formulário — cadastrar um produto rápido (nome + NCM + grupo/unidade) não deveria
    // exigir rolar por eles; ficam atrás de uma `SecaoExpansivel`, fechada por padrão.
    ui.add_space(Espaco::E12);
    SecaoExpansivel::nova("Estoque inicial (opcional)").mostrar(ui, |ui| {
        bloco_local(ui, motor, sessao, estado);
        ui.add_space(Espaco::E8);
        ui.columns(2, |c| {
            c[0].add(Campo::novo("Quantidade", &mut estado.estoque_inicial_qtd));
            c[1].add(
                Campo::novo("Custo unitário", &mut estado.estoque_inicial_custo).marcador("90,00"),
            );
        });
    });

    ui.add_space(Espaco::E8);
    SecaoExpansivel::nova("Detalhes técnicos (opcional)")
        .mostrar(ui, |ui| bloco_detalhes_tecnicos(ui, estado));
}

/// Os campos de detalhes técnicos — compartilhado entre "Novo produto" (`corpo_novo`) e
/// "Editar detalhes técnicos" (`dialogo_editar`), que edita exatamente os mesmos campos de
/// `DetalhesTecnicos` sobre um produto já cadastrado.
fn bloco_detalhes_tecnicos(ui: &mut egui::Ui, estado: &mut EstadoTelaEstoque) {
    // TODO(backend): campo de código curto de rastreabilidade (post-it físico) ainda
    // não existe em `DetalhesTecnicos`/`estoque_produto` — quando existir, entra aqui
    // como mais um `Campo` (ex.: "Código de bancada") e já alimenta a busca da lista
    // e o lookup de peça na abertura de OS (ver `tela_os::adicionar_peca`).
    ui.columns(2, |c| {
        c[0].add(
            Campo::novo("Fabricante", &mut estado.det_fabricante)
                .marcador("ex.: Texas Instruments"),
        );
        c[1].add(
            Campo::novo(
                "Código do fabricante (MPN)",
                &mut estado.det_codigo_fabricante,
            )
            .marcador("part number"),
        );
    });
    ui.add_space(Espaco::E8);
    ui.columns(2, |c| {
        c[0].add(
            Campo::novo("Categoria técnica", &mut estado.det_categoria_tecnica)
                .marcador("IC, capacitor, tela, bateria…"),
        );
        c[1].add(
            Campo::novo(
                "Garantia do fornecedor (dias)",
                &mut estado.det_garantia_fornecedor_dias,
            )
            .marcador("90"),
        );
    });
    ui.add_space(Espaco::E8);
    ui.add(
        Campo::novo(
            "Especificação / resumo do datasheet",
            &mut estado.det_especificacao_tecnica,
        )
        .marcador("tensão, corrente, pinagem…"),
    );
    ui.add_space(Espaco::E8);
    ui.columns(2, |c| {
        c[0].add(
            Campo::novo(
                "Compatibilidade / aplicação",
                &mut estado.det_compatibilidade,
            )
            .marcador("ex.: iPhone 11 / 11 Pro"),
        );
        c[1].add(
            Campo::novo("Localização física", &mut estado.det_localizacao_fisica)
                .marcador("ex.: Gaveta 12, prateleira B"),
        );
    });
}

/// Monta `DetalhesTecnicos` a partir dos campos de texto do formulário — `None` por campo
/// vazio, e `None` no conjunto inteiro se nada foi preenchido (mesma regra de `cadastrar`).
fn montar_detalhes_tecnicos(estado: &EstadoTelaEstoque) -> Option<DetalhesTecnicos> {
    let d = DetalhesTecnicos {
        fabricante: (!estado.det_fabricante.trim().is_empty())
            .then(|| estado.det_fabricante.clone()),
        codigo_fabricante: (!estado.det_codigo_fabricante.trim().is_empty())
            .then(|| estado.det_codigo_fabricante.clone()),
        categoria_tecnica: (!estado.det_categoria_tecnica.trim().is_empty())
            .then(|| estado.det_categoria_tecnica.clone()),
        especificacao_tecnica: (!estado.det_especificacao_tecnica.trim().is_empty())
            .then(|| estado.det_especificacao_tecnica.clone()),
        compatibilidade: (!estado.det_compatibilidade.trim().is_empty())
            .then(|| estado.det_compatibilidade.clone()),
        garantia_fornecedor_dias: estado.det_garantia_fornecedor_dias.trim().parse().ok(),
        localizacao_fisica: (!estado.det_localizacao_fisica.trim().is_empty())
            .then(|| estado.det_localizacao_fisica.clone()),
    };
    (d != DetalhesTecnicos::default()).then_some(d)
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
                Err(e) => notificar(ui.ctx(), Notificacao::erro(e.mensagem)),
            }
        }
    } else {
        let ops: Vec<(Id, String)> = estado
            .grupos
            .iter()
            .map(|g| (g.id, g.nome.clone()))
            .collect();
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
        ui.add(Campo::novo(
            "Nome da unidade",
            &mut estado.nova_unidade_nome,
        ));
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
                Err(e) => notificar(ui.ctx(), Notificacao::erro(e.mensagem)),
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
                Err(e) => notificar(ui.ctx(), Notificacao::erro(e.mensagem)),
            }
        }
    } else {
        let ops: Vec<(Id, String)> = estado
            .locais
            .iter()
            .map(|l| (l.id, l.nome.clone()))
            .collect();
        SeletorOpcao::novo("Local", &mut estado.local_selecionado)
            .opcoes(ops)
            .mostrar(ui);
    }
}

fn cadastrar(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaEstoque,
) {
    let detalhes_tecnicos = montar_detalhes_tecnicos(estado);

    let r = motor.executar(
        sessao,
        "estoque.criar_produto.v1",
        &CriarProduto {
            grupo_produto: estado.grupo_selecionado.unwrap_or(Id::NULO),
            nome: estado.produto_nome.clone(),
            ncm: estado.produto_ncm.clone(),
            unidade_padrao: estado.unidade_selecionada.unwrap_or(Id::NULO),
            codigo_barras: (!estado.produto_codigo_barras.trim().is_empty())
                .then(|| estado.produto_codigo_barras.clone()),
            detalhes_tecnicos,
        },
    );
    let criado: ProdutoCriado = match r {
        Ok(c) => c,
        Err(e) => {
            notificar(ctx, Notificacao::erro(e.mensagem));
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
            notificar(
                ctx,
                Notificacao::aviso("Produto criado, mas o estoque inicial falhou")
                    .detalhe(e.mensagem),
            );
        }
    }

    estado.limpar_novo();
    estado.dlg = Dlg::Fechado;
    estado.carregar(motor, sessao);
    notificar(ctx, Notificacao::sucesso("Produto cadastrado"));
}
