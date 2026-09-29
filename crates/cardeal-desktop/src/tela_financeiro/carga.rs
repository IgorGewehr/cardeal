//! O que a tela de Financeiro busca no motor: painel, parcelas do período, fluxo, análise e
//! a geração de recorrências ao entrar.

use super::*;

impl EstadoTelaFinanceiro {
    /// Gera os títulos das recorrências vencidas ou dentro da antecedência
    /// (`financeiro.materializar_recorrencias.v1`, idempotente) e depois recarrega. É o que
    /// roda ao entrar no sistema e ao abrir a área de Financeiro — até existir agendador, é o
    /// que faz o aluguel cadastrado como recorrente aparecer em "A pagar".
    pub fn gerar_recorrencias_e_carregar(&mut self, motor: &MotorLocal, sessao: &SessaoLocal) {
        if sessao.concede("financeiro.recorrencia.criar") {
            match motor.executar(
                sessao,
                "financeiro.materializar_recorrencias.v1",
                &mod_financeiro::MaterializarRecorrencias,
            ) {
                Ok(gerados) => {
                    let gerados: Vec<Id> = gerados;
                    self.recorrencias_geradas += gerados.len();
                }
                Err(e) => {
                    self.carregar(motor, sessao);
                    self.erro = Some(format!("Recorrências não geradas: {}", e.mensagem));
                    return;
                }
            }
        }
        self.carregar(motor, sessao);
    }

    /// Guarda o erro de uma consulta para aparecer no topo da tela (em vez de o valor virar
    /// zero em silêncio) e devolve o valor quando deu certo.
    pub(super) fn anotar<T>(&mut self, r: cardeal_kernel::Resultado<T>) -> Option<T> {
        match r {
            Ok(v) => Some(v),
            Err(e) => {
                self.erro = Some(e.mensagem);
                None
            }
        }
    }

    /// Recarrega a lista da aba ativa, o painel de visão geral e o índice de nomes.
    pub fn carregar(&mut self, motor: &MotorLocal, sessao: &SessaoLocal) {
        self.erro = None;
        // Nomes primeiro: o extrato monta (e guarda) "OS #123 — cliente" com eles; carregar
        // depois deixava o rótulo sem o nome para sempre.
        self.carregar_nomes(motor, sessao);
        self.carregar_categorias(motor, sessao);
        // O painel antes das parcelas: a projeção reaproveita o "em aberto" dele.
        self.carregar_dashboard(motor, sessao);
        if matches!(self.aba, Aba::Receber | Aba::Pagar) {
            self.carregar_parcelas_periodo(motor, sessao);
        }
        if matches!(self.aba, Aba::Fluxo | Aba::Bancos) {
            self.carregar_fluxo(motor, sessao);
        }
    }

    /// Clientes e fornecedores (seletores do lançamento e o índice `nomes`).
    pub(super) fn carregar_nomes(&mut self, motor: &MotorLocal, sessao: &SessaoLocal) {
        if let Ok(c) = motor.consultar(
            sessao,
            "clientes.pessoas_por_papel.v1",
            &PessoasPorPapel {
                papel: Papel::Cliente,
                busca: None,
            },
        ) {
            self.clientes = c;
        }
        if let Ok(f) = motor.consultar(
            sessao,
            "clientes.pessoas_por_papel.v1",
            &PessoasPorPapel {
                papel: Papel::Fornecedor,
                busca: None,
            },
        ) {
            self.fornecedores = f;
        }
        self.nomes = self
            .clientes
            .iter()
            .chain(self.fornecedores.iter())
            .map(|p| (p.pessoa, p.nome.clone()))
            .collect();
    }

    /// Recarrega a listagem (qualquer estado, dentro do período selecionado) e os dois
    /// cards da aba ativa (Receber/Pagar): quanto foi baixado no período, e a projeção do
    /// saldo em aberto que já vence até o fim do período.
    pub(super) fn carregar_parcelas_periodo(&mut self, motor: &MotorLocal, sessao: &SessaoLocal) {
        let hoje = Data::hoje(Fuso::BRASILIA);
        let periodo = self.periodo.resolver(hoje);
        let a_receber = self.aba.a_receber();

        let r = if a_receber {
            motor.consultar(
                sessao,
                "financeiro.parcelas_a_receber_no_periodo.v1",
                &ParcelasAReceberNoPeriodo { periodo },
            )
        } else {
            motor.consultar(
                sessao,
                "financeiro.parcelas_a_pagar_no_periodo.v1",
                &ParcelasAPagarNoPeriodo { periodo },
            )
        };
        match r {
            Ok(p) => self.parcelas = p,
            Err(e) => self.erro = Some(e.mensagem),
        }
        self.carregar_origens(motor, sessao);

        let baixado = if a_receber {
            motor.consultar(
                sessao,
                "financeiro.total_recebido_no_periodo.v1",
                &TotalRecebidoNoPeriodo { periodo },
            )
        } else {
            motor.consultar(
                sessao,
                "financeiro.total_pago_no_periodo.v1",
                &TotalPagoNoPeriodo { periodo },
            )
        };
        // Um total que falhou não vira "R$ 0,00": o erro aparece no topo da tela.
        self.baixado_periodo = self.anotar(baixado).unwrap_or(Dinheiro::ZERO);

        // Projeção: soma do saldo em aberto (qualquer vencimento, inclusive já vencido) que
        // já vence até o fim do período — reaproveita o "em aberto" que o painel já carregou
        // (`carregar_dashboard` roda antes), só filtrado aqui pela data-fim.
        let abertos = if a_receber {
            &self.abertos_receber
        } else {
            &self.abertos_pagar
        };
        self.projecao_periodo = abertos
            .iter()
            .filter(|p| p.vencimento <= periodo.ate)
            .fold(Dinheiro::ZERO, |acc, p| acc + p.saldo());
    }

    /// Alimenta o painel de visão geral: saldos em aberto, o que vence em 7 dias e a série
    /// de recebido × pago dos últimos 6 meses.
    pub(super) fn carregar_dashboard(&mut self, motor: &MotorLocal, sessao: &SessaoLocal) {
        let hoje = Data::hoje(Fuso::BRASILIA);
        let em7 = hoje.mais_dias(7);

        let receber = motor.consultar(
            sessao,
            "financeiro.titulos_a_receber_em_aberto.v1",
            &TitulosAReceberEmAberto,
        );
        let receber: Vec<ItemTituloEmAberto> = self.anotar(receber).unwrap_or_default();
        let pagar = motor.consultar(
            sessao,
            "financeiro.titulos_a_pagar_em_aberto.v1",
            &TitulosAPagarEmAberto,
        );
        let pagar: Vec<ItemTituloEmAberto> = self.anotar(pagar).unwrap_or_default();

        let soma = |v: &[ItemTituloEmAberto]| {
            v.iter()
                .map(ItemTituloEmAberto::saldo)
                .fold(Dinheiro::ZERO, |a, b| a + b)
        };
        let venc = |v: &[ItemTituloEmAberto]| {
            let atrasa_ou_vence: Vec<&ItemTituloEmAberto> =
                v.iter().filter(|p| p.vencimento <= em7).collect();
            let total = atrasa_ou_vence
                .iter()
                .map(|p| p.saldo())
                .fold(Dinheiro::ZERO, |a, b| a + b);
            (atrasa_ou_vence.len(), total)
        };
        self.dash_receber = soma(&receber);
        self.dash_pagar = soma(&pagar);
        self.dash_venc_receber = venc(&receber);
        self.dash_venc_pagar = venc(&pagar);
        self.abertos_receber = receber;
        self.abertos_pagar = pagar;

        let periodo = Periodo::novo(hoje.mais_meses(-5).inicio_do_mes(), hoje);
        let itens = motor.consultar(
            sessao,
            "financeiro.total_por_categoria_no_periodo.v1",
            &TotalPorCategoriaNoPeriodo { periodo },
        );
        let itens: Vec<ItemTotalPorCategoria> = self.anotar(itens).unwrap_or_default();

        let mut comp = hoje.mais_meses(-5).competencia();
        let mut serie = Vec::with_capacity(6);
        for _ in 0..6 {
            let total = |a_receber: bool| -> Dinheiro {
                itens
                    .iter()
                    .filter(|it| {
                        it.competencia == comp
                            && matches!(it.especie, EspecieTitulo::Receber) == a_receber
                    })
                    .fold(Dinheiro::ZERO, |acc, it| acc + it.total_baixado)
            };
            serie.push(MesFluxo {
                rotulo: nome_mes(comp),
                receita: total(true),
                custo: total(false),
            });
            comp = comp.proxima();
        }
        self.dash_recebido_mes = serie.last().map_or(Dinheiro::ZERO, |m| m.receita);
        self.dash_pago_mes = serie.last().map_or(Dinheiro::ZERO, |m| m.custo);
        self.serie = serie;

        self.dash_margem_os = motor
            .consultar(
                sessao,
                "os.margem_das_ordens_no_periodo.v1",
                &mod_os::MargemDasOrdensNoPeriodo {
                    periodo: Periodo::novo(hoje.inicio_do_mes(), hoje),
                },
            )
            .ok();

        // Também alimenta o recorte "por categoria" já visível na Visão Geral (antes só
        // aparecia depois de clicar em "Ver por categoria") — mesma janela de 6 meses.
        self.carregar_analise(motor, sessao);
    }

    /// As 5 categorias de maior movimento (receita − despesa, em módulo) nos últimos
    /// [`Self::analise_meses`] meses — o recorte "Por categoria" da Visão Geral.
    pub(super) fn top_categorias(&self) -> Vec<cardeal_ui::organisms::ItemBarraHorizontal> {
        let mut por_cat: HashMap<Option<Id>, f64> = HashMap::new();
        for it in &self.analise {
            let sinal = match it.especie {
                EspecieTitulo::Receber => 1.0,
                EspecieTitulo::Pagar => -1.0,
            };
            #[allow(clippy::cast_precision_loss)]
            let valor = it.total_baixado.em_centavos() as f64 / 100.0 * sinal;
            *por_cat.entry(it.categoria).or_insert(0.0) += valor;
        }
        let mut itens: Vec<cardeal_ui::organisms::ItemBarraHorizontal> = por_cat
            .into_iter()
            .map(|(cat, valor)| cardeal_ui::organisms::ItemBarraHorizontal {
                rotulo: self.nome_categoria(cat),
                valor,
            })
            .collect();
        itens.sort_by(|a, b| b.valor.abs().total_cmp(&a.valor.abs()));
        itens.truncate(5);
        itens
    }

    pub(super) fn carregar_fluxo(&mut self, motor: &MotorLocal, sessao: &SessaoLocal) {
        if self.fluxo_dias == 0 {
            self.fluxo_dias = 30;
        }
        let hoje = Data::hoje(Fuso::BRASILIA);
        let periodo = Periodo::novo(
            hoje.mais_dias(-i32::try_from(self.fluxo_dias).unwrap_or(30)),
            hoje,
        );
        match motor.consultar(
            sessao,
            "financeiro.extrato_disponivel.v1",
            &ExtratoDisponivel {
                periodo,
                conta: None,
            },
        ) {
            Ok(v) => {
                self.extrato = v;
                self.carregar_origens(motor, sessao);
            }
            Err(e) => self.erro = Some(e.mensagem),
        }
        match motor.consultar(
            sessao,
            "financeiro.contas_disponiveis.v1",
            &ContasDisponiveis,
        ) {
            Ok(v) => self.contas_disp = v,
            Err(e) => self.erro = Some(e.mensagem),
        }
    }

    /// Resolve "OS #123 — cliente" para cada OS por trás do extrato e das parcelas da aba
    /// (`self.origem_labels`, consumido por [`rotulo_origem`] e pela coluna Origem). Só `"os"`
    /// precisa de consulta — as outras origens se rotulam pelo módulo.
    pub(super) fn carregar_origens(&mut self, motor: &MotorLocal, sessao: &SessaoLocal) {
        let do_extrato = self
            .extrato
            .iter()
            .filter(|m| m.origem_modulo.as_deref() == Some("os"))
            .filter_map(|m| m.origem_id);
        let das_parcelas = self
            .parcelas
            .iter()
            .filter(|p| p.origem_modulo == "os")
            .filter_map(|p| p.origem_id);
        let mut ids: Vec<Id> = do_extrato
            .chain(das_parcelas)
            .filter(|id| !self.origem_labels.contains_key(id))
            .collect();
        if ids.is_empty() {
            return;
        }
        ids.sort_unstable();
        ids.dedup();
        // Uma consulta para todas as OS (antes era um detalhe completo por linha). Sem
        // permissão de ver OS, fica o rótulo genérico "OS", sem erro na tela.
        let ordens: Result<Vec<mod_os::OrdemServico>, _> = motor.consultar(
            sessao,
            "os.ordens_por_id.v1",
            &mod_os::OrdensPorId { ordens: ids },
        );
        for os in ordens.unwrap_or_default() {
            let rotulo = match self.nomes.get(&os.cliente) {
                Some(nome) => format!("OS #{} — {nome}", os.numero),
                None => format!("OS #{}", os.numero),
            };
            self.origem_labels.insert(os.id, rotulo);
        }
    }

    /// O rótulo de origem de um movimento do extrato, se resolvido (ver
    /// [`carregar_origens`]).
    pub(super) fn rotulo_origem(&self, m: &ItemMovimentoDisponivel) -> Option<&str> {
        m.origem_id
            .and_then(|id| self.origem_labels.get(&id))
            .map(String::as_str)
    }

    pub(super) fn carregar_analise(&mut self, motor: &MotorLocal, sessao: &SessaoLocal) {
        if self.analise_meses == 0 {
            self.analise_meses = 6;
        }
        let hoje = Data::hoje(Fuso::BRASILIA);
        let periodo = Periodo::novo(hoje.mais_meses(-i32::from(self.analise_meses)), hoje);
        match motor.consultar(
            sessao,
            "financeiro.total_por_categoria_no_periodo.v1",
            &TotalPorCategoriaNoPeriodo { periodo },
        ) {
            Ok(v) => self.analise = v,
            Err(e) => self.erro = Some(e.mensagem),
        }
    }

    pub(super) fn carregar_recorrencias(&mut self, motor: &MotorLocal, sessao: &SessaoLocal) {
        match motor.consultar(sessao, "financeiro.recorrencias.v1", &Recorrencias) {
            Ok(v) => self.recorrencias = v,
            Err(e) => self.erro = Some(e.mensagem),
        }
    }

    /// Contas de resultado da espécie pedida — Receitas p/ recorrência a receber, Despesas
    /// p/ a pagar. O seletor de `conta_contrapartida` do formulário.
    pub(super) fn carregar_contas(
        &mut self,
        motor: &MotorLocal,
        sessao: &SessaoLocal,
        a_receber: bool,
    ) {
        let especie = if a_receber {
            EspecieTitulo::Receber
        } else {
            EspecieTitulo::Pagar
        };
        match motor.consultar(
            sessao,
            "financeiro.contas_de_resultado.v1",
            &ContasDeResultado { especie },
        ) {
            Ok(v) => {
                self.contas = v;
                self.contas_receber = a_receber;
            }
            Err(e) => self.erro = Some(e.mensagem),
        }
    }

    /// Recarrega só as categorias — usado depois de criar uma nova, sem repetir o resto do
    /// que `carregar` busca.
    pub(super) fn carregar_categorias(&mut self, motor: &MotorLocal, sessao: &SessaoLocal) {
        match motor.consultar(sessao, "financeiro.categorias.v1", &Categorias) {
            Ok(v) => self.categorias = v,
            Err(e) => self.erro = Some(e.mensagem),
        }
    }

    /// Índices de `parcelas` cujo nome de contraparte bate com `busca` — filtro client-side,
    /// mesmo racional de `tela_estoque.rs::produtos_filtrados`: a consulta já trouxe até 500
    /// linhas de uma vez, e o volume de uma assistência cabe folgado nisso.
    pub(super) fn parcelas_filtradas(&self, situacao: FiltroParcelas, hoje: Data) -> Vec<usize> {
        let termo = self.busca.trim().to_lowercase();
        (0..self.parcelas.len())
            .filter(|&i| {
                let p = &self.parcelas[i];
                situacao.combina(p, hoje)
                    && (termo.is_empty() || self.texto_busca(p).contains(&termo))
            })
            .collect()
    }

    /// O que a busca da lista compara: contraparte, descrição e origem ("OS #12 — Maria",
    /// "Compra"…), em minúsculas.
    fn texto_busca(&self, p: &ItemTituloEmAberto) -> String {
        format!(
            "{} {} {}",
            self.nome_contraparte(&p.contraparte),
            p.descricao.as_deref().unwrap_or_default(),
            self.origem_da_parcela(p).0
        )
        .to_lowercase()
    }

    pub(super) fn nome_categoria(&self, id: Option<Id>) -> String {
        id.and_then(|i| self.categorias.iter().find(|c| c.id == i))
            .map_or_else(|| "Sem categoria".to_owned(), |c| c.nome.clone())
    }

    pub(super) fn nome_contraparte(&self, c: &Option<Contraparte>) -> String {
        let Some(c) = c else {
            return "Sem cliente/fornecedor".to_owned();
        };
        let id = match c {
            Contraparte::Cliente(i)
            | Contraparte::Fornecedor(i)
            | Contraparte::Funcionario(i)
            | Contraparte::Socio(i)
            | Contraparte::Outro(i) => *i,
        };
        self.nomes
            .get(&id)
            .cloned()
            .unwrap_or_else(|| "—".to_owned())
    }
}
