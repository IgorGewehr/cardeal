//! As abas "A receber" / "A pagar": período, indicadores e a grade de parcelas.

use super::*;
use cardeal_ui::atoms::Caixa;
use cardeal_ui::organisms::RespostaGrade;

/// O seletor de período das abas "A receber"/"A pagar" — presets de calendário + uma faixa
/// digitada (`PresetPeriodo::Personalizado`). Devolve `true` quando o período mudou e a
/// tela precisa recarregar.
pub(super) fn seletor_periodo(ui: &mut egui::Ui, filtro: &mut FiltroPeriodo) -> bool {
    let mut mudou = false;
    ui.horizontal(|ui| {
        ui.add(Rotulo::sobrelinha("Período"));
        ui.add_space(Espaco::E8);
        let novo = Abas::nova(&[
            (PresetPeriodo::Dia, "Dia"),
            (PresetPeriodo::Semana, "Semana"),
            (PresetPeriodo::Mes, "Mês"),
            (PresetPeriodo::Trimestre, "Trimestre"),
            (PresetPeriodo::Semestre, "Semestre"),
            (PresetPeriodo::Ano, "Ano"),
            (PresetPeriodo::Personalizado, "Personalizado"),
        ])
        .selecionada(filtro.preset)
        .id_salt("financeiro-periodo")
        .mostrar(ui);
        if let Some(preset) = novo {
            filtro.preset = preset;
            if preset == PresetPeriodo::Personalizado {
                let hoje = Data::hoje(Fuso::BRASILIA).to_string();
                if filtro.de_personalizado.is_empty() {
                    filtro.de_personalizado = hoje.clone();
                }
                if filtro.ate_personalizado.is_empty() {
                    filtro.ate_personalizado = hoje;
                }
            } else {
                mudou = true;
            }
        }
    });
    if filtro.preset == PresetPeriodo::Personalizado {
        ui.add_space(Espaco::E8);
        ui.horizontal(|ui| {
            ui.add(Campo::novo("De", &mut filtro.de_personalizado).mascara(Mascara::Data));
            ui.add_space(Espaco::E8);
            ui.add(Campo::novo("Até", &mut filtro.ate_personalizado).mascara(Mascara::Data));
            ui.add_space(Espaco::E8);
            if ui.add(Botao::secundario("Aplicar")).clicked() {
                mudou = true;
            }
        });
    }
    mudou
}

pub(super) fn lista(
    ui: &mut egui::Ui,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaFinanceiro,
) {
    if seletor_periodo(ui, &mut estado.periodo) {
        estado.carregar_parcelas_periodo(motor, sessao);
    }
    ui.add_space(Espaco::E12);

    let hoje = Data::hoje(Fuso::BRASILIA);
    let periodo = estado.periodo.resolver(hoje);
    let a_receber = estado.aba.a_receber();

    // Os dois cards pedidos explicitamente pelo usuário: quanto foi de fato recebido/pago
    // no período (baixas, não vencimento) e a projeção do que ainda vai vencer — nunca
    // some da tela mesmo com a lista vazia, pra sempre dar pra ver "nada aconteceu nesse
    // período" com números, não um vazio mudo.
    let vencidas = estado
        .parcelas
        .iter()
        .filter(|p| p.estado.aceita_baixa() && p.vencimento < hoje)
        .count();
    // Dinheiro que sai é vermelho; que entra, verde.
    let tom = if a_receber {
        Tom::Positivo
    } else {
        Tom::Negativo
    };
    FaixaKpi::nova(vec![
        CartaoKpi::novo(
            if a_receber {
                "Recebido no período"
            } else {
                "Pago no período"
            },
            estado.baixado_periodo,
        )
        .tom(tom),
        CartaoKpi::contagem("Vencidas", vencidas),
        CartaoKpi::novo(
            if a_receber {
                format!("A receber até {}", periodo.ate.formatar_curta())
            } else {
                format!("A pagar até {}", periodo.ate.formatar_curta())
            },
            estado.projecao_periodo,
        )
        .tom(tom),
    ])
    .mostrar(ui);
    ui.add_space(Espaco::E16);

    if estado.parcelas.is_empty() {
        let msg = if a_receber {
            "Nada a receber nesse período."
        } else {
            "Nada a pagar nesse período."
        };
        EstadoVazio::novo(Icone::Dinheiro, msg).mostrar(ui);
        return;
    }

    let opcoes_categoria: Vec<(FiltroCategoria, String)> = [
        (FiltroCategoria::Todas, "Todas as categorias".to_owned()),
        (FiltroCategoria::Sem, "Sem categoria".to_owned()),
    ]
    .into_iter()
    .chain(
        estado
            .categorias
            .iter()
            .map(|c| (FiltroCategoria::Uma(c.id), c.nome.clone())),
    )
    .collect();
    BarraFiltros::nova(&mut estado.busca)
        .marcador("Buscar por nome, descrição, categoria ou origem (OS #12…)")
        .filtro(|ui| {
            SeletorOpcao::novo("Categoria", &mut estado.filtro_categoria)
                .sem_rotulo()
                .placeholder("Todas as categorias")
                .opcoes(opcoes_categoria)
                .mostrar(ui);
        })
        .filtro(|ui| {
            SeletorOpcao::novo("Situação", &mut estado.filtro_situacao)
                .sem_rotulo()
                .placeholder("Todas as parcelas")
                .opcao(FiltroParcelas::Todas, "Todas as parcelas")
                .opcao(FiltroParcelas::EmAberto, "Em aberto")
                .opcao(FiltroParcelas::Vencidas, "Vencidas")
                .opcao(
                    FiltroParcelas::Quitadas,
                    if a_receber { "Recebidas" } else { "Pagas" },
                )
                .mostrar(ui);
        })
        .acao(|ui| {
            // "Receber/Pagar várias" liga o modo de seleção; ligado, a faixa abaixo assume.
            if estado.selecao.is_none() {
                let rotulo = if a_receber {
                    "Receber várias"
                } else {
                    "Pagar várias"
                };
                if ui.add(Botao::secundario(rotulo).pequeno()).clicked() {
                    estado.selecao = Some(Vec::new());
                }
            }
        })
        .mostrar(ui);

    let situacao = estado.filtro_situacao.unwrap_or(FiltroParcelas::Todas);
    let indices = estado.parcelas_filtradas(situacao, hoje);
    barra_selecao(ui, estado);
    let selecionando = estado.selecao.is_some();
    // Com a coluna da caixa de seleção na frente, os índices de coluna andam uma casa.
    let desloc = usize::from(selecionando);
    // "Valor" é o da parcela; o que já entrou/saiu e o que falta ficam em colunas próprias —
    // antes só existia "Saldo", e uma parcela quitada aparecia como R$ 0,00, apagando o valor
    // real que ela teve.
    let rotulo_baixado = if a_receber { "Recebido" } else { "Pago" };
    let mut colunas = Vec::with_capacity(9);
    if selecionando {
        colunas.push(ColunaGrade::nova("").largura(40.0));
    }
    colunas.extend([
        ColunaGrade::nova(if a_receber { "Cliente" } else { "Favorecido" }).largura(170.0),
        ColunaGrade::nova("Categoria").largura(150.0),
        ColunaGrade::nova("Origem").largura(170.0),
        ColunaGrade::nova("Descrição"),
        ColunaGrade::nova("Parc.").largura(60.0).numero(),
        ColunaGrade::nova("Vencimento").largura(120.0),
        ColunaGrade::nova("Valor").largura(130.0).numero(),
        ColunaGrade::nova(rotulo_baixado).largura(130.0).numero(),
        ColunaGrade::nova("Saldo").largura(130.0).numero(),
        ColunaGrade::nova("Estado").largura(120.0),
    ]);
    let mut marcada_na_caixa = None;
    let resposta = Grade::nova(colunas)
        .selecionavel(None)
        .ordenacao(estado.ordenacao.atual().map(|(c, d)| (c + desloc, d)))
        .vazio("Nenhuma parcela para essa busca.")
        .mostrar(ui, indices.len(), |i, row| {
            let p = &estado.parcelas[indices[i]];
            if let Some(ids) = &estado.selecao {
                row.col(|ui| {
                    if p.estado.aceita_baixa() {
                        let mut marcada = ids.contains(&p.parcela);
                        if ui.add(Caixa::nova(&mut marcada, "")).changed() {
                            marcada_na_caixa = Some(i);
                        }
                    }
                });
            }
            row.col(|ui| {
                let nome = estado.nome_contraparte(&p.contraparte);
                if nome == "—" {
                    ui.add(Rotulo::interface("—").cor(ui.cores().texto_fraco));
                } else {
                    ui.add(Rotulo::interface(nome));
                }
            });
            row.col(|ui| {
                if p.categoria.is_some() {
                    ui.add(Rotulo::interface(estado.nome_categoria(p.categoria)));
                } else {
                    ui.add(Rotulo::interface("—").cor(ui.cores().texto_fraco));
                }
            });
            row.col(|ui| {
                ui.add(estado.etiqueta_origem(p));
            });
            row.col(|ui| match p.descricao.as_deref().map(str::trim) {
                Some(d) if !d.is_empty() => {
                    ui.add(Rotulo::interface(d.to_owned()).cor(ui.cores().texto_medio));
                }
                _ => {
                    ui.add(Rotulo::interface("—").cor(ui.cores().texto_fraco));
                }
            });
            row.col(|ui| {
                ui.add(Rotulo::interface(p.numero.to_string()));
            });
            // Vermelho só para o que está em aberto e passou do prazo: uma parcela já
            // recebida com vencimento antigo não é problema nenhum.
            let vencida = p.estado.aceita_baixa() && p.vencimento < hoje;
            row.col(|ui| {
                let r = Rotulo::interface(p.vencimento.to_string());
                ui.add(if vencida {
                    r.cor(ui.cores().negativo)
                } else {
                    r
                });
            });
            row.col(|ui| {
                ui.add(ValorDinheiro::novo(p.valor_original).neutro());
            });
            row.col(|ui| {
                if p.valor_baixado.e_zero() {
                    ui.add(Rotulo::interface("—").cor(ui.cores().texto_fraco));
                } else {
                    ui.add(ValorDinheiro::novo(p.valor_baixado));
                }
            });
            row.col(|ui| {
                // O saldo só faz sentido enquanto a parcela está em aberto.
                if p.estado.aceita_baixa() && !p.saldo().e_zero() {
                    ui.add(ValorDinheiro::novo(p.saldo()).neutro());
                } else {
                    ui.add(Rotulo::interface("—").cor(ui.cores().texto_fraco));
                }
            });
            row.col(|ui| {
                ui.add(etiqueta_estado_parcela(p.estado, vencida, a_receber));
            });
        });

    if let Some(coluna) = resposta.coluna_clicada {
        if coluna >= desloc {
            let resposta = RespostaGrade {
                coluna_clicada: Some(coluna - desloc),
                ..resposta
            };
            if let Some((coluna, direcao)) = estado.ordenacao.clicar(&resposta) {
                let nomes = estado.nomes.clone();
                let categorias: HashMap<Id, String> = estado
                    .categorias
                    .iter()
                    .map(|c| (c.id, c.nome.clone()))
                    .collect();
                ordenar_parcelas(&mut estado.parcelas, &nomes, &categorias, coluna, direcao);
            }
        }
    }
    // Selecionando, o clique na linha (ou na caixa) marca/desmarca em vez de abrir a baixa.
    if selecionando {
        if let Some(i) = marcada_na_caixa.or(resposta.linha_clicada) {
            let p = &estado.parcelas[indices[i]];
            if p.estado.aceita_baixa() {
                let id = p.parcela;
                if let Some(ids) = &mut estado.selecao {
                    if let Some(pos) = ids.iter().position(|x| *x == id) {
                        ids.remove(pos);
                    } else {
                        ids.push(id);
                    }
                }
            }
        }
        return;
    }
    if let Some(i) = resposta.linha_clicada {
        let p = &estado.parcelas[indices[i]];
        // Em aberto abre para dar baixa; quitada abre para consulta (histórico e estorno).
        // Cancelada/renegociada não têm o que fazer aqui.
        if matches!(
            p.estado,
            EstadoParcela::Aberta | EstadoParcela::Parcial | EstadoParcela::Quitada
        ) {
            estado.dlg = Dlg::Baixar {
                parcela: p.parcela,
                valor: p.saldo().formatar(),
                data: Data::hoje(Fuso::BRASILIA).to_string(),
                pagamento: crate::pagamento::EstadoPagamento::novo(MeioPagamento::Pix),
                baixas: Vec::new(),
                baixas_carregadas: false,
                motivo_estorno: String::new(),
                situacao: None,
                valor_sugerido: String::new(),
                renegociar: FormRenegociar::default(),
            };
        }
    }
}

/// Etiqueta colorida para o estado de uma parcela — a lista mostra qualquer estado agora
/// (`parcelas_no_periodo` não filtra), então além de `Aberta`/`Parcial` também chega
/// `Quitada`/`Cancelada`/`Renegociada` (rótulo neutro, caso padrão). Vencida pesa mais que
/// o estado em si, por isso entra primeiro na escolha do tom. Quitada é verde e diz "Recebida"
/// ou "Paga" conforme a aba — o que o usuário quer ler, não o termo do backend.
pub(super) fn etiqueta_estado_parcela(
    estado: EstadoParcela,
    vencida: bool,
    a_receber: bool,
) -> Etiqueta {
    match (estado, vencida) {
        (EstadoParcela::Quitada, _) => {
            Etiqueta::positiva(if a_receber { "Recebida" } else { "Paga" })
        }
        (EstadoParcela::Parcial, true) => Etiqueta::negativa("Parcial · vencida"),
        (EstadoParcela::Parcial, false) => Etiqueta::info("Parcial"),
        (EstadoParcela::Aberta, true) => Etiqueta::negativa("Vencida"),
        (EstadoParcela::Aberta, false) => Etiqueta::neutra("Aberta"),
        (outro, _) => Etiqueta::neutra(outro.rotulo()),
    }
}

/// Ordena `parcelas` pela coluna clicada no cabeçalho da [`Grade`] de `lista` (mesma ordem
/// das colunas: Contraparte, Categoria, Origem, Descrição, Parc., Vencimento, Valor, Recebido/Pago, Saldo, Estado). `nomes` resolve o nome de
/// exibição da contraparte — a própria coluna 0 ordena por ele, não pelo id.
pub(super) fn ordenar_parcelas(
    parcelas: &mut [ItemTituloEmAberto],
    nomes: &HashMap<Id, String>,
    categorias: &HashMap<Id, String>,
    coluna: usize,
    direcao: Direcao,
) {
    let nome_de = |p: &ItemTituloEmAberto| -> String {
        let Some(c) = p.contraparte else {
            return String::new();
        };
        let id = match c {
            Contraparte::Cliente(i)
            | Contraparte::Fornecedor(i)
            | Contraparte::Funcionario(i)
            | Contraparte::Socio(i)
            | Contraparte::Outro(i) => i,
        };
        nomes.get(&id).cloned().unwrap_or_default()
    };
    let cat_de = |p: &ItemTituloEmAberto| {
        p.categoria
            .and_then(|c| categorias.get(&c))
            .cloned()
            .unwrap_or_default()
    };
    parcelas.sort_by(|a, b| {
        let ordem = match coluna {
            0 => nome_de(a).cmp(&nome_de(b)),
            1 => cat_de(a).cmp(&cat_de(b)),
            2 => a.origem_modulo.cmp(&b.origem_modulo),
            3 => a.descricao.cmp(&b.descricao),
            4 => a.numero.cmp(&b.numero),
            5 => a.vencimento.cmp(&b.vencimento),
            6 => a.valor_original.cmp(&b.valor_original),
            7 => a.valor_baixado.cmp(&b.valor_baixado),
            8 => a.saldo().cmp(&b.saldo()),
            9 => a.estado.rotulo().cmp(b.estado.rotulo()),
            _ => std::cmp::Ordering::Equal,
        };
        match direcao {
            Direcao::Ascendente => ordem,
            Direcao::Descendente => ordem.reverse(),
        }
    });
}

/// A faixa acima da grade para quitar várias parcelas de uma vez: "Baixar várias" liga o
/// modo de seleção; com ele ligado, mostra quantas e quanto estão marcadas e o botão que abre
/// [`Dlg::BaixarLote`].
fn barra_selecao(ui: &mut egui::Ui, estado: &mut EstadoTelaFinanceiro) {
    let Some(ids) = &estado.selecao else { return };
    let quantas = ids.len();
    let total = estado
        .parcelas
        .iter()
        .filter(|p| ids.contains(&p.parcela))
        .fold(Dinheiro::ZERO, |acc, p| acc + p.saldo());
    Painel::novo()
        .realce(Tom::Info)
        .compacto()
        .mostrar(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add(Rotulo::interface(format!(
                    "{quantas} selecionada(s) · {}",
                    total.formatar_com_simbolo()
                )));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add(
                            Botao::primario("Baixar selecionadas")
                                .pequeno()
                                .habilitado(quantas > 0),
                        )
                        .clicked()
                    {
                        estado.dlg = Dlg::BaixarLote {
                            data: Data::hoje(Fuso::BRASILIA).to_string(),
                            pagamento: crate::pagamento::EstadoPagamento::novo(MeioPagamento::Pix),
                        };
                    }
                    if ui
                        .add(Botao::fantasma("Cancelar seleção").pequeno())
                        .clicked()
                    {
                        estado.selecao = None;
                    }
                });
            });
        });
    ui.add_space(Espaco::E8);
}

impl EstadoTelaFinanceiro {
    /// De onde veio a parcela, para a coluna Origem: a OS com o cliente, a compra, o PDV, a
    /// recorrência ou o lançamento manual — o que separa receita de serviço, custo de peça e
    /// despesa fixa sem abrir nada.
    pub(super) fn origem_da_parcela(&self, p: &ItemTituloEmAberto) -> (String, Tom) {
        let (texto, tom) = match p.origem_modulo.as_str() {
            "os" => {
                let os = p.origem_id.and_then(|id| self.origem_labels.get(&id));
                return (os.cloned().unwrap_or_else(|| "OS".to_owned()), Tom::Info);
            }
            "os_peca" => ("Peça de OS", Tom::Atencao),
            "compras" => ("Compra", Tom::Atencao),
            "pdv" => ("PDV", Tom::Positivo),
            "vendas" => ("Venda", Tom::Positivo),
            "estoque" => ("Estoque", Tom::Neutro),
            "financeiro_recorrencia" => ("Recorrência", Tom::Neutro),
            _ => ("Manual", Tom::Neutro),
        };
        (texto.to_owned(), tom)
    }

    pub(super) fn etiqueta_origem(&self, p: &ItemTituloEmAberto) -> Etiqueta {
        let (texto, tom) = self.origem_da_parcela(p);
        Etiqueta::nova(texto, tom)
    }
}
