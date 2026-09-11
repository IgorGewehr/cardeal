//! Lucratividade real de uma ordem de serviço: receita (mão de obra + peça cobradas) menos
//! custo real (peça pelo `custo_unitario` real do estoque, mão de obra pelo tempo apontado
//! × custo/hora do técnico, quando configurado).
//!
//! Domínio puro: recebe os dados já lidos pelas consultas públicas de `mod-os`
//! ([`mod_os::DetalheOrdem`], [`mod_os::ApontamentoDeTempo`]) — nunca lê `os_*` direto
//! (`docs/contratos-internos.md` §7 regra 2: um crate só enxerga outro módulo pela sua
//! interface pública, nunca pela tabela). O cálculo em si não tem I/O nenhum, por isso é
//! inteiramente testável com dados sintéticos.
//!
//! **Custo de mão de obra é opcional por desenho.** Sem uma tabela de custo/hora por técnico
//! ainda cadastrada no sistema, não existe base para inventar um número — por isso
//! [`MargemDeOs::custo_mao_de_obra`]/[`MargemDeOs::margem_liquida`] são `Option`: `None`
//! significa explicitamente "não calculável com o que está configurado hoje", nunca "zero".
//! A margem **bruta** (só peça) é sempre calculável, porque o custo real da peça já vem do
//! estoque de verdade (`ItemPeca::custo_unitario`, gravado por `AplicarPeca`).

use std::collections::BTreeMap;

use cardeal_kernel::{Arredondamento, Dinheiro, Id};
use mod_os::{ApontamentoDeTempo, DetalheOrdem, ItemPeca};
use serde::{Deserialize, Serialize};

/// O custo por hora de cada técnico — o dado que falta hoje para transformar tempo apontado
/// em custo de mão de obra. Configurável por fora (uma tela futura de cadastro de técnicos);
/// por ora é só um mapa em memória que o chamador monta.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustoHorario(BTreeMap<Id, Dinheiro>);

impl CustoHorario {
    /// Uma tabela de custo/hora vazia — nenhum técnico calculável.
    #[must_use]
    pub fn nova() -> Self {
        Self(BTreeMap::new())
    }

    /// Registra (ou substitui) o custo por hora de um técnico.
    #[must_use]
    pub fn com_tecnico(mut self, tecnico: Id, valor_por_hora: Dinheiro) -> Self {
        self.0.insert(tecnico, valor_por_hora);
        self
    }

    /// O custo por hora de um técnico, se cadastrado.
    #[must_use]
    pub fn valor_hora(&self, tecnico: Id) -> Option<Dinheiro> {
        self.0.get(&tecnico).copied()
    }
}

/// A lucratividade calculada de uma ordem de serviço.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MargemDeOs {
    /// A ordem de serviço.
    pub ordem_servico: Id,
    /// O número sequencial, para exibição.
    pub numero: u64,
    /// Quanto foi cobrado do cliente pelas peças (zero nos itens cobertos por garantia).
    pub receita_pecas: Dinheiro,
    /// Quanto foi cobrado do cliente pela mão de obra orçada.
    pub receita_mao_de_obra: Dinheiro,
    /// O custo real das peças efetivamente aplicadas (do estoque, não do orçamento).
    pub custo_pecas: Dinheiro,
    /// O tempo total apontado (só apontamentos encerrados), em segundos.
    pub tempo_apontado_segundos: i64,
    /// O custo da mão de obra real (tempo apontado × custo/hora do técnico) — `None` quando
    /// algum técnico que trabalhou na OS não tem custo/hora cadastrado.
    pub custo_mao_de_obra: Option<Dinheiro>,
    /// `receita_total - custo_pecas` — sempre calculável.
    pub margem_bruta: Dinheiro,
    /// `margem_bruta - custo_mao_de_obra` — `None` quando `custo_mao_de_obra` é `None`.
    pub margem_liquida: Option<Dinheiro>,
}

impl MargemDeOs {
    /// A receita total (peça + mão de obra) cobrada do cliente.
    #[must_use]
    pub fn receita_total(&self) -> Dinheiro {
        self.receita_pecas + self.receita_mao_de_obra
    }
}

/// A duração (em segundos) de um apontamento **encerrado** — mesma regra de
/// `mod_os::consultas::tempo_total_da_ordem`: um apontamento ainda aberto está em
/// andamento e não entra na margem (evita um número que muda a cada consulta).
fn duracao_encerrada_segundos(ap: &ApontamentoDeTempo) -> i64 {
    ap.fim
        .map_or(0, |fim| ap.inicio.micros_ate(fim).max(0) / 1_000_000)
}

/// Soma o tempo encerrado por técnico.
fn segundos_por_tecnico(apontamentos: &[ApontamentoDeTempo]) -> BTreeMap<Id, i64> {
    let mut por_tecnico: BTreeMap<Id, i64> = BTreeMap::new();
    for ap in apontamentos.iter().filter(|a| !a.esta_aberto()) {
        *por_tecnico.entry(ap.tecnico).or_default() += duracao_encerrada_segundos(ap);
    }
    por_tecnico
}

/// Converte segundos de trabalho num custo, dado o valor da hora — `Dinheiro::fracao`
/// preserva a exatidão em centavos (`docs/contratos-internos.md` §1: nada de `f64` para
/// dinheiro).
fn custo_do_tempo(segundos: i64, valor_por_hora: Dinheiro) -> Dinheiro {
    valor_por_hora.fracao(segundos, 3600, Arredondamento::MeioAcima)
}

/// Calcula a margem de uma ordem de serviço a partir do detalhe (já obtido via
/// `mod_os::BuscarDetalheOrdem`) e dos apontamentos de tempo (via
/// `mod_os::ApontamentosDaOrdem`).
#[must_use]
pub fn calcular_margem_de_os(
    detalhe: &DetalheOrdem,
    apontamentos: &[ApontamentoDeTempo],
    custo_horario: &CustoHorario,
) -> MargemDeOs {
    let receita_pecas: Dinheiro = detalhe.itens_peca.iter().map(ItemPeca::total_cobrado).sum();
    let receita_mao_de_obra: Dinheiro = detalhe.itens_mao_de_obra.iter().map(|i| i.valor).sum();
    let custo_pecas: Dinheiro = detalhe.itens_peca.iter().map(ItemPeca::total_custo).sum();

    let tempo_apontado_segundos: i64 = apontamentos
        .iter()
        .filter(|a| !a.esta_aberto())
        .map(duracao_encerrada_segundos)
        .sum();

    let margem_bruta = receita_pecas + receita_mao_de_obra - custo_pecas;

    let por_tecnico = segundos_por_tecnico(apontamentos);
    let custo_mao_de_obra = if por_tecnico.is_empty() {
        // Nenhum tempo apontado ainda: custo de mão de obra real é zero, não "desconhecido".
        Some(Dinheiro::ZERO)
    } else {
        por_tecnico
            .iter()
            .try_fold(Dinheiro::ZERO, |acc, (&tecnico, &segundos)| {
                custo_horario
                    .valor_hora(tecnico)
                    .map(|valor_hora| acc + custo_do_tempo(segundos, valor_hora))
            })
    };
    let margem_liquida = custo_mao_de_obra.map(|c| margem_bruta - c);

    MargemDeOs {
        ordem_servico: detalhe.ordem.id,
        numero: detalhe.ordem.numero,
        receita_pecas,
        receita_mao_de_obra,
        custo_pecas,
        tempo_apontado_segundos,
        custo_mao_de_obra,
        margem_bruta,
        margem_liquida,
    }
}

/// A lucratividade agregada de um conjunto de ordens (ex.: todas as faturadas num mês) — a
/// base de um dashboard futuro.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MargemAgregada {
    /// Quantas ordens entraram na soma.
    pub quantidade_os: usize,
    /// A receita total (peça + mão de obra) das ordens.
    pub receita_total: Dinheiro,
    /// O custo real de peça total.
    pub custo_pecas_total: Dinheiro,
    /// A margem bruta total (sempre calculável).
    pub margem_bruta_total: Dinheiro,
    /// O custo de mão de obra total — `None` se QUALQUER ordem do grupo tiver mão de obra
    /// não calculável (um técnico sem custo/hora cadastrado é suficiente para tornar o
    /// agregado inteiro não confiável, em vez de fingir que a parcela dele é zero).
    pub custo_mao_de_obra_total: Option<Dinheiro>,
    /// A margem líquida total — `None` nas mesmas condições de `custo_mao_de_obra_total`.
    pub margem_liquida_total: Option<Dinheiro>,
    /// O tempo total apontado do grupo, em segundos.
    pub tempo_total_segundos: i64,
}

/// Agrega a margem de várias ordens — por período (mês/semana), por técnico, ou qualquer
/// outro recorte que o chamador já tenha filtrado antes de passar o slice.
#[must_use]
pub fn agregar_margens(margens: &[MargemDeOs]) -> MargemAgregada {
    let receita_total = margens.iter().map(MargemDeOs::receita_total).sum();
    let custo_pecas_total = margens.iter().map(|m| m.custo_pecas).sum();
    let margem_bruta_total = margens.iter().map(|m| m.margem_bruta).sum();
    let tempo_total_segundos = margens.iter().map(|m| m.tempo_apontado_segundos).sum();

    let custo_mao_de_obra_total = margens.iter().try_fold(Dinheiro::ZERO, |acc, m| {
        m.custo_mao_de_obra.map(|c| acc + c)
    });
    let margem_liquida_total = margens
        .iter()
        .try_fold(Dinheiro::ZERO, |acc, m| m.margem_liquida.map(|l| acc + l));

    MargemAgregada {
        quantidade_os: margens.len(),
        receita_total,
        custo_pecas_total,
        margem_bruta_total,
        custo_mao_de_obra_total,
        margem_liquida_total,
        tempo_total_segundos,
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use cardeal_kernel::{Instante, Preco, Quantidade};
    use mod_os::{EstadoOs, OrdemServico};

    fn ordem(id: Id, numero: u64) -> OrdemServico {
        OrdemServico {
            id,
            empresa: Id::novo(),
            numero,
            cliente: Id::novo(),
            equipamento: "Notebook".to_string(),
            defeito_relatado: "Não liga".to_string(),
            data_abertura: cardeal_kernel::Data::de_dias(20_000),
            tecnico_responsavel: Id::novo(),
            estado: EstadoOs::Faturada,
            aprovado_por: Some("Cliente".to_string()),
            garantia_dias: 90,
            valor_total: Dinheiro::ZERO,
            itens_orcamento: 1,
            versao: cardeal_kernel::Versao::INICIAL,
        }
    }

    fn peca(ordem_servico: Id, preco_unitario: Preco, custo_unitario: Preco) -> ItemPeca {
        let mut item = ItemPeca::novo(
            ordem_servico,
            Id::novo(),
            Quantidade::unidades(1),
            preco_unitario,
        );
        item.aplicar(custo_unitario).unwrap();
        item
    }

    fn detalhe(os: OrdemServico, itens_peca: Vec<ItemPeca>) -> DetalheOrdem {
        DetalheOrdem {
            ordem: os,
            laudo: None,
            itens_peca,
            itens_mao_de_obra: Vec::new(),
        }
    }

    fn ap_encerrado(ordem_servico: Id, tecnico: Id, segundos: i64) -> ApontamentoDeTempo {
        let mut a = ApontamentoDeTempo::iniciar(ordem_servico, tecnico, Instante::EPOCA);
        a.encerrar(Instante::EPOCA.mais_segundos(segundos)).unwrap();
        a
    }

    #[test]
    fn os_com_boa_margem_de_peca() {
        let id = Id::novo();
        let os = ordem(id, 1);
        let d = detalhe(os, vec![peca(id, Preco::reais(160), Preco::reais(90))]);
        let m = calcular_margem_de_os(&d, &[], &CustoHorario::nova());
        assert_eq!(m.receita_pecas, Dinheiro::reais(160));
        assert_eq!(m.custo_pecas, Dinheiro::reais(90));
        assert_eq!(m.margem_bruta, Dinheiro::reais(70));
        // Sem apontamento nenhum: custo de mão de obra é zero, não "desconhecido".
        assert_eq!(m.custo_mao_de_obra, Some(Dinheiro::ZERO));
        assert_eq!(m.margem_liquida, Some(Dinheiro::reais(70)));
    }

    #[test]
    fn os_no_prejuizo_quando_peca_foi_mal_orcada() {
        let id = Id::novo();
        let os = ordem(id, 2);
        // Cobrou 50, mas o custo real da peça no estoque era 80 — orçamento errado.
        let d = detalhe(os, vec![peca(id, Preco::reais(50), Preco::reais(80))]);
        let m = calcular_margem_de_os(&d, &[], &CustoHorario::nova());
        assert_eq!(m.margem_bruta, Dinheiro::reais(-30));
        assert!(m.margem_bruta.e_negativo());
    }

    #[test]
    fn os_de_garantia_com_peca_a_custo_zero_para_o_cliente() {
        let id = Id::novo();
        let os = ordem(id, 3);
        let mut item = peca(id, Preco::ZERO, Preco::reais(90));
        item.coberto_garantia = true;
        let d = detalhe(os, vec![item]);
        let m = calcular_margem_de_os(&d, &[], &CustoHorario::nova());
        assert_eq!(m.receita_pecas, Dinheiro::ZERO); // coberto por garantia não cobra
        assert_eq!(m.custo_pecas, Dinheiro::reais(90)); // mas o custo real existiu
        assert_eq!(m.margem_bruta, Dinheiro::reais(-90));
    }

    #[test]
    fn custo_de_mao_de_obra_nao_calculavel_sem_custo_hora_do_tecnico() {
        let id = Id::novo();
        let os = ordem(id, 4);
        let tecnico = Id::novo();
        let d = detalhe(os, vec![peca(id, Preco::reais(160), Preco::reais(90))]);
        let apontamentos = vec![ap_encerrado(id, tecnico, 3600)];
        // Nenhum custo/hora cadastrado para este técnico.
        let m = calcular_margem_de_os(&d, &apontamentos, &CustoHorario::nova());
        assert_eq!(m.tempo_apontado_segundos, 3600);
        assert_eq!(m.custo_mao_de_obra, None);
        assert_eq!(m.margem_liquida, None);
        // A margem bruta (só peça) continua calculável mesmo sem custo/hora.
        assert_eq!(m.margem_bruta, Dinheiro::reais(70));
    }

    #[test]
    fn custo_de_mao_de_obra_calculado_quando_custo_hora_esta_configurado() {
        let id = Id::novo();
        let os = ordem(id, 5);
        let tecnico = Id::novo();
        let d = detalhe(os, vec![peca(id, Preco::reais(160), Preco::reais(90))]);
        // Duas horas de trabalho a R$ 40,00/hora = R$ 80,00 de custo de mão de obra.
        let apontamentos = vec![ap_encerrado(id, tecnico, 7200)];
        let custos = CustoHorario::nova().com_tecnico(tecnico, Dinheiro::reais(40));
        let m = calcular_margem_de_os(&d, &apontamentos, &custos);
        assert_eq!(m.custo_mao_de_obra, Some(Dinheiro::reais(80)));
        assert_eq!(
            m.margem_liquida,
            Some(Dinheiro::reais(70) - Dinheiro::reais(80))
        );
        assert!(m.margem_liquida.unwrap().e_negativo()); // deu prejuízo com a mão de obra real
    }

    #[test]
    fn apontamento_aberto_nao_entra_na_soma() {
        let id = Id::novo();
        let os = ordem(id, 6);
        let tecnico = Id::novo();
        let d = detalhe(os, Vec::new());
        let mut aberto = ApontamentoDeTempo::iniciar(id, tecnico, Instante::EPOCA);
        assert!(aberto.esta_aberto());
        let m = calcular_margem_de_os(&d, std::slice::from_ref(&aberto), &CustoHorario::nova());
        assert_eq!(m.tempo_apontado_segundos, 0);
        // Encerra e recalcula: agora entra.
        aberto
            .encerrar(Instante::EPOCA.mais_segundos(1800))
            .unwrap();
        let m2 = calcular_margem_de_os(&d, &[aberto], &CustoHorario::nova());
        assert_eq!(m2.tempo_apontado_segundos, 1800);
    }

    #[test]
    fn agregacao_soma_varias_ordens_e_propaga_nao_calculavel() {
        let id1 = Id::novo();
        let id2 = Id::novo();
        let d1 = detalhe(
            ordem(id1, 1),
            vec![peca(id1, Preco::reais(160), Preco::reais(90))],
        );
        let d2 = detalhe(
            ordem(id2, 2),
            vec![peca(id2, Preco::reais(50), Preco::reais(80))],
        );

        let m1 = calcular_margem_de_os(&d1, &[], &CustoHorario::nova());
        let m2 = calcular_margem_de_os(&d2, &[], &CustoHorario::nova());
        let agregada = agregar_margens(&[m1, m2]);

        assert_eq!(agregada.quantidade_os, 2);
        assert_eq!(agregada.receita_total, Dinheiro::reais(210)); // 160 + 50
        assert_eq!(agregada.custo_pecas_total, Dinheiro::reais(170)); // 90 + 80
        assert_eq!(agregada.margem_bruta_total, Dinheiro::reais(40)); // 70 + (-30)
        assert_eq!(agregada.margem_liquida_total, Some(Dinheiro::reais(40)));

        // Se uma das ordens tem mão de obra não calculável, o agregado inteiro fica None.
        let tecnico = Id::novo();
        let m3 = calcular_margem_de_os(
            &d1,
            &[ap_encerrado(id1, tecnico, 3600)],
            &CustoHorario::nova(),
        );
        assert_eq!(m3.custo_mao_de_obra, None);
        let agregada2 = agregar_margens(&[m3]);
        assert_eq!(agregada2.custo_mao_de_obra_total, None);
        assert_eq!(agregada2.margem_liquida_total, None);
    }
}
