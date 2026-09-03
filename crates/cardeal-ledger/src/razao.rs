//! As operações do Razão: registrar, estornar, confirmar, liquidar.
//!
//! Cada função é genérica sobre [`PortaRazao`] — não sabe se está falando com SQLite ou
//! com um double em memória. Ver `docs/05-nucleo-financeiro.md` §3.4 e §6.
//!
//! **Fora de escopo nesta etapa:** `fechar_periodo`/`reabrir_periodo` (exigem somar saldos
//! de todas as contas na data — uma consulta histórica, que só faz sentido junto do
//! adaptador de persistência real) e as consultas de saldo/fluxo/DRE. Ver
//! `docs/17-roadmap.md`.

use cardeal_kernel::{Data, Id};

use crate::construtor::{ConstrutorLancamento, LancamentoBalanceado};
use crate::erros::ErroRazao;
use crate::lancamento::EstadoLancamento;
use crate::porta::PortaRazao;

/// Comprimento mínimo, em caracteres, do motivo de um estorno — curto demais não serve
/// como trilha de auditoria (`docs/05-nucleo-financeiro.md` §6).
const MOTIVO_MINIMO: usize = 10;

/// Namespace das operações do Razão. Sem estado próprio: tudo vive na [`PortaRazao`].
pub struct Razao;

impl Razao {
    /// Valida e grava um lançamento já balanceado.
    ///
    /// O que [`LancamentoBalanceado`] garante por construção (soma zero, autoria, mínimo de
    /// partidas) não precisa ser checado de novo aqui. O que só a porta sabe responder —
    /// se a conta existe, está ativa, é analítica, pertence à mesma empresa, e se o período
    /// está aberto — é checado agora, antes de atribuir o número e gravar.
    ///
    /// # Errors
    /// Ver [`ErroRazao`]: `ContaNaoEncontrada`, `ContaInativa`, `ContaSintetica`,
    /// `EmpresaDivergente`, `PeriodoFechado`.
    pub fn registrar<P: PortaRazao>(
        porta: &mut P,
        lancamento: LancamentoBalanceado,
    ) -> Result<Id, ErroRazao> {
        let mut lancamento = lancamento.dentro();

        if let Some(fechado_ate) = porta.periodo_fechado_ate(lancamento.empresa)? {
            if lancamento.competencia <= fechado_ate {
                return Err(ErroRazao::PeriodoFechado { ate: fechado_ate });
            }
        }

        for partida in &lancamento.partidas {
            let info = porta
                .info_conta(partida.conta)?
                .ok_or(ErroRazao::ContaNaoEncontrada(partida.conta))?;
            if info.empresa != lancamento.empresa {
                return Err(ErroRazao::EmpresaDivergente);
            }
            if !info.ativa {
                return Err(ErroRazao::ContaInativa(partida.conta));
            }
            if matches!(info.tipo, crate::conta::TipoConta::Sintetica) {
                return Err(ErroRazao::ContaSintetica {
                    conta: partida.conta,
                    codigo: info.codigo,
                });
            }
        }

        lancamento.numero = porta.proximo_numero_lancamento(lancamento.empresa)?;
        let id = lancamento.id;
        porta.inserir_lancamento(lancamento)?;
        Ok(id)
    }

    /// Estorna um lançamento: cria um espelho com sinais invertidos, marca o original
    /// como [`EstadoLancamento::Estornado`] e vincula os dois nos dois sentidos. Nada é
    /// apagado nem sobrescrito — ver `docs/05-nucleo-financeiro.md` §6.
    ///
    /// O estorno herda o estado do original (estornar um `Previsto` gera outro `Previsto`;
    /// estornar um `Realizado` — dinheiro que já andou — gera outro `Realizado`, para que o
    /// saldo de caixa reflita a devolução de verdade).
    ///
    /// Usuário e dispositivo vêm da própria `porta` (a unidade de trabalho corrente), não
    /// como parâmetro — é quem está executando o comando *agora*, não quem criou o
    /// lançamento original.
    ///
    /// # Errors
    /// [`ErroRazao::MotivoDeEstornoCurto`] se `motivo` tiver menos de 10 caracteres,
    /// [`ErroRazao::LancamentoNaoEncontrado`], [`ErroRazao::JaEstornado`],
    /// [`ErroRazao::PeriodoFechado`] se `competencia` cair em período fechado.
    pub fn estornar<P: PortaRazao>(
        porta: &mut P,
        original: Id,
        motivo: &str,
        competencia: Data,
    ) -> Result<Id, ErroRazao> {
        let motivo = motivo.trim();
        if motivo.chars().count() < MOTIVO_MINIMO {
            return Err(ErroRazao::MotivoDeEstornoCurto);
        }

        let mut original_lanc = porta
            .buscar_lancamento(original)?
            .ok_or(ErroRazao::LancamentoNaoEncontrado(original))?;
        if matches!(original_lanc.estado, EstadoLancamento::Estornado) {
            return Err(ErroRazao::JaEstornado(original));
        }
        if let Some(fechado_ate) = porta.periodo_fechado_ate(original_lanc.empresa)? {
            if competencia <= fechado_ate {
                return Err(ErroRazao::PeriodoFechado { ate: fechado_ate });
            }
        }

        let estado_original = original_lanc.estado;
        let mut construtor = ConstrutorLancamento::novo(
            original_lanc.empresa,
            competencia,
            format!("Estorno do lançamento {} — {motivo}", original_lanc.numero),
        )
        .origem(original_lanc.origem.clone())
        .estado(estado_original)
        .criado_por(porta.usuario(), porta.dispositivo())
        .agora(porta.agora());

        if let Some(liq) = original_lanc.liquidacao {
            construtor = construtor.liquidacao(liq.max(competencia));
        }

        for partida in &original_lanc.partidas {
            let invertido = -partida.valor;
            construtor = if invertido.e_positivo() {
                construtor.debitar(partida.conta, invertido)
            } else {
                construtor.creditar(partida.conta, invertido.abs())
            };
            if let Some(cp) = partida.contraparte {
                construtor = construtor.contraparte(cp);
            }
            if let Some(cc) = partida.centro_custo {
                construtor = construtor.centro_custo(cc);
            }
            if let Some(pr) = partida.projeto {
                construtor = construtor.projeto(pr);
            }
            if let Some(doc) = &partida.documento {
                construtor = construtor.documento(doc.clone());
            }
            if let Some(q) = partida.quantidade {
                construtor = construtor.quantidade(q);
            }
        }

        // O construtor já provou que o espelho também balanceia — é aritmética: se as
        // partidas originais somam zero, suas negativas também somam zero.
        let mut estorno = construtor.construir()?.dentro();
        estorno.estorna = Some(original);
        estorno.numero = porta.proximo_numero_lancamento(estorno.empresa)?;
        let estorno_id = estorno.id;

        // O espelho entra primeiro: só então o original pode apontar `estornado_por` para
        // ele sem violar a integridade referencial (`razao_lancamento.estornado_por`).
        porta.inserir_lancamento(estorno)?;
        original_lanc.estado = EstadoLancamento::Estornado;
        original_lanc.estornado_por = Some(estorno_id);
        porta.atualizar_lancamento(original_lanc)?;

        Ok(estorno_id)
    }

    /// Faz um lançamento [`EstadoLancamento::Previsto`] avançar para
    /// [`EstadoLancamento::Confirmado`] — o fato ocorreu, mas o dinheiro ainda não andou.
    ///
    /// # Errors
    /// [`ErroRazao::LancamentoNaoEncontrado`], [`ErroRazao::TransicaoInvalida`] se o
    /// lançamento não estiver em `Previsto`.
    pub fn confirmar<P: PortaRazao>(porta: &mut P, id: Id) -> Result<(), ErroRazao> {
        let mut lancamento = porta
            .buscar_lancamento(id)?
            .ok_or(ErroRazao::LancamentoNaoEncontrado(id))?;
        if lancamento.estado != EstadoLancamento::Previsto {
            return Err(ErroRazao::TransicaoInvalida {
                de: lancamento.estado,
                para: EstadoLancamento::Confirmado,
            });
        }
        lancamento.estado = EstadoLancamento::Confirmado;
        porta.atualizar_lancamento(lancamento)?;
        Ok(())
    }

    /// Faz um lançamento avançar para [`EstadoLancamento::Realizado`] — o dinheiro andou
    /// em `quando`. Válido a partir de `Previsto` ou `Confirmado`.
    ///
    /// # Errors
    /// [`ErroRazao::LancamentoNaoEncontrado`], [`ErroRazao::TransicaoInvalida`] se o
    /// lançamento já estiver `Realizado` ou `Estornado`.
    pub fn liquidar<P: PortaRazao>(porta: &mut P, id: Id, quando: Data) -> Result<(), ErroRazao> {
        let mut lancamento = porta
            .buscar_lancamento(id)?
            .ok_or(ErroRazao::LancamentoNaoEncontrado(id))?;
        if !matches!(
            lancamento.estado,
            EstadoLancamento::Previsto | EstadoLancamento::Confirmado
        ) {
            return Err(ErroRazao::TransicaoInvalida {
                de: lancamento.estado,
                para: EstadoLancamento::Realizado,
            });
        }
        lancamento.estado = EstadoLancamento::Realizado;
        lancamento.liquidacao = Some(quando);
        porta.atualizar_lancamento(lancamento)?;
        Ok(())
    }
}

#[cfg(test)]
mod testes {
    use cardeal_kernel::{Dinheiro, Fuso};

    use super::*;
    use crate::conta::{PapelConta, TipoConta};
    use crate::contas::Contas;
    use crate::testkit_interno::RazaoEmMemoria;

    fn hoje() -> Data {
        Data::hoje(Fuso::BRASILIA)
    }

    fn lancar_venda(mundo: &mut RazaoEmMemoria, empresa: Id, caixa: Id, receita: Id) -> Id {
        let lanc = ConstrutorLancamento::novo(empresa, hoje(), "Venda de teste")
            .liquidacao(hoje())
            .criado_por(mundo.usuario(), mundo.dispositivo())
            .debitar(caixa, Dinheiro::reais(100))
            .creditar(receita, Dinheiro::reais(100))
            .construir()
            .unwrap();
        Razao::registrar(mundo, lanc).unwrap()
    }

    #[test]
    fn registrar_grava_e_atribui_numero() {
        let mut mundo = RazaoEmMemoria::nova();
        let empresa = Id::novo();
        let caixa = mundo.semear_plano_padrao(empresa);
        let receita = Contas::nova(&mundo, empresa)
            .papel(PapelConta::ReceitaVendas)
            .unwrap();

        let id = lancar_venda(&mut mundo, empresa, caixa, receita);
        let gravado = mundo.obter(id);
        assert_eq!(gravado.numero, 1);
        assert_eq!(gravado.estado, EstadoLancamento::Realizado);

        // Um segundo lançamento recebe o próximo número da sequência da empresa.
        let id2 = lancar_venda(&mut mundo, empresa, caixa, receita);
        assert_eq!(mundo.obter(id2).numero, 2);
    }

    #[test]
    fn registrar_recusa_conta_sintetica() {
        let mut mundo = RazaoEmMemoria::nova();
        let empresa = Id::novo();
        mundo.semear_plano_padrao(empresa);
        // "1" é sintética no plano padrão.
        let sintetica = Contas::nova(&mundo, empresa).por_codigo("1").unwrap();
        let outra = Contas::nova(&mundo, empresa).por_codigo("1.1.02").unwrap();

        let lanc = ConstrutorLancamento::novo(empresa, hoje(), "inválido")
            .criado_por(mundo.usuario(), mundo.dispositivo())
            .debitar(sintetica, Dinheiro::reais(10))
            .creditar(outra, Dinheiro::reais(10))
            .construir()
            .unwrap();

        let erro = Razao::registrar(&mut mundo, lanc).unwrap_err();
        assert!(matches!(erro, ErroRazao::ContaSintetica { .. }));
    }

    #[test]
    fn registrar_recusa_conta_inativa() {
        let mut mundo = RazaoEmMemoria::nova();
        let empresa = Id::novo();
        let ativa = mundo.inserir_conta(empresa, "1.1.01", TipoConta::Analitica, true, None);
        let inativa = mundo.inserir_conta(empresa, "1.1.02", TipoConta::Analitica, false, None);

        let lanc = ConstrutorLancamento::novo(empresa, hoje(), "inválido")
            .criado_por(mundo.usuario(), mundo.dispositivo())
            .debitar(ativa, Dinheiro::reais(10))
            .creditar(inativa, Dinheiro::reais(10))
            .construir()
            .unwrap();

        let erro = Razao::registrar(&mut mundo, lanc).unwrap_err();
        assert!(matches!(erro, ErroRazao::ContaInativa(id) if id == inativa));
    }

    #[test]
    fn registrar_recusa_periodo_fechado() {
        let mut mundo = RazaoEmMemoria::nova();
        let empresa = Id::novo();
        let caixa = mundo.semear_plano_padrao(empresa);
        let receita = Contas::nova(&mundo, empresa)
            .papel(PapelConta::ReceitaVendas)
            .unwrap();
        mundo.fechar_periodo(empresa, hoje());

        let lanc = ConstrutorLancamento::novo(empresa, hoje(), "período fechado")
            .criado_por(mundo.usuario(), mundo.dispositivo())
            .debitar(caixa, Dinheiro::reais(10))
            .creditar(receita, Dinheiro::reais(10))
            .construir()
            .unwrap();

        let erro = Razao::registrar(&mut mundo, lanc).unwrap_err();
        assert!(matches!(erro, ErroRazao::PeriodoFechado { .. }));

        // Reaberto, o mesmo lançamento passa.
        mundo.reabrir_periodo(empresa);
        let lanc2 = ConstrutorLancamento::novo(empresa, hoje(), "período reaberto")
            .criado_por(mundo.usuario(), mundo.dispositivo())
            .debitar(caixa, Dinheiro::reais(10))
            .creditar(receita, Dinheiro::reais(10))
            .construir()
            .unwrap();
        assert!(Razao::registrar(&mut mundo, lanc2).is_ok());
    }

    #[test]
    fn estorno_zera_o_saldo_das_contas_envolvidas() {
        let mut mundo = RazaoEmMemoria::nova();
        let empresa = Id::novo();
        let caixa = mundo.semear_plano_padrao(empresa);
        let receita = Contas::nova(&mundo, empresa)
            .papel(PapelConta::ReceitaVendas)
            .unwrap();

        let venda = lancar_venda(&mut mundo, empresa, caixa, receita);
        let estorno_id =
            Razao::estornar(&mut mundo, venda, "cliente devolveu a mercadoria", hoje()).unwrap();

        let original = mundo.obter(venda);
        let estorno = mundo.obter(estorno_id);

        assert_eq!(original.estado, EstadoLancamento::Estornado);
        assert_eq!(original.estornado_por, Some(estorno_id));
        assert_eq!(estorno.estorna, Some(venda));
        // O espelho tem sinais invertidos: onde a venda debitou, o estorno credita.
        assert_eq!(estorno.soma(), Dinheiro::ZERO);
        let soma_combinada: Dinheiro = original
            .partidas
            .iter()
            .chain(estorno.partidas.iter())
            .map(|p| p.valor)
            .sum();
        assert_eq!(
            soma_combinada,
            Dinheiro::ZERO,
            "original + estorno deve zerar por conta"
        );
    }

    #[test]
    fn estorno_exige_motivo_com_dez_caracteres() {
        let mut mundo = RazaoEmMemoria::nova();
        let empresa = Id::novo();
        let caixa = mundo.semear_plano_padrao(empresa);
        let receita = Contas::nova(&mundo, empresa)
            .papel(PapelConta::ReceitaVendas)
            .unwrap();
        let venda = lancar_venda(&mut mundo, empresa, caixa, receita);

        let erro = Razao::estornar(&mut mundo, venda, "curto", hoje()).unwrap_err();
        assert!(matches!(erro, ErroRazao::MotivoDeEstornoCurto));
    }

    #[test]
    fn nao_e_possivel_estornar_duas_vezes() {
        let mut mundo = RazaoEmMemoria::nova();
        let empresa = Id::novo();
        let caixa = mundo.semear_plano_padrao(empresa);
        let receita = Contas::nova(&mundo, empresa)
            .papel(PapelConta::ReceitaVendas)
            .unwrap();
        let venda = lancar_venda(&mut mundo, empresa, caixa, receita);

        Razao::estornar(&mut mundo, venda, "motivo válido, dez+", hoje()).unwrap();
        let erro = Razao::estornar(&mut mundo, venda, "motivo válido, dez+", hoje()).unwrap_err();
        assert!(matches!(erro, ErroRazao::JaEstornado(id) if id == venda));
    }

    #[test]
    fn confirmar_so_avanca_de_previsto() {
        let mut mundo = RazaoEmMemoria::nova();
        let empresa = Id::novo();
        let caixa = mundo.semear_plano_padrao(empresa);
        let receita = Contas::nova(&mundo, empresa)
            .papel(PapelConta::ReceitaVendas)
            .unwrap();

        let previsto = ConstrutorLancamento::novo(empresa, hoje(), "projeção")
            .estado(EstadoLancamento::Previsto)
            .criado_por(mundo.usuario(), mundo.dispositivo())
            .debitar(caixa, Dinheiro::reais(10))
            .creditar(receita, Dinheiro::reais(10))
            .construir()
            .unwrap();
        let id = Razao::registrar(&mut mundo, previsto).unwrap();

        Razao::confirmar(&mut mundo, id).unwrap();
        assert_eq!(mundo.obter(id).estado, EstadoLancamento::Confirmado);

        // Confirmar de novo (já Confirmado, não Previsto) falha.
        let erro = Razao::confirmar(&mut mundo, id).unwrap_err();
        assert!(matches!(erro, ErroRazao::TransicaoInvalida { .. }));
    }

    #[test]
    fn liquidar_marca_realizado_e_a_data() {
        let mut mundo = RazaoEmMemoria::nova();
        let empresa = Id::novo();
        let caixa = mundo.semear_plano_padrao(empresa);
        let receita = Contas::nova(&mundo, empresa)
            .papel(PapelConta::ReceitaVendas)
            .unwrap();

        let confirmado = ConstrutorLancamento::novo(empresa, hoje(), "a prazo")
            .vencimento(hoje().mais_dias(30))
            .criado_por(mundo.usuario(), mundo.dispositivo())
            .debitar(caixa, Dinheiro::reais(10))
            .creditar(receita, Dinheiro::reais(10))
            .construir()
            .unwrap();
        let id = Razao::registrar(&mut mundo, confirmado).unwrap();
        assert_eq!(mundo.obter(id).estado, EstadoLancamento::Confirmado);

        let quando = hoje().mais_dias(30);
        Razao::liquidar(&mut mundo, id, quando).unwrap();
        let liquidado = mundo.obter(id);
        assert_eq!(liquidado.estado, EstadoLancamento::Realizado);
        assert_eq!(liquidado.liquidacao, Some(quando));

        // Liquidar um lançamento já realizado não é uma transição válida.
        let erro = Razao::liquidar(&mut mundo, id, quando).unwrap_err();
        assert!(matches!(erro, ErroRazao::TransicaoInvalida { .. }));
    }

    #[test]
    fn nao_e_possivel_confirmar_ou_liquidar_lancamento_estornado() {
        let mut mundo = RazaoEmMemoria::nova();
        let empresa = Id::novo();
        let caixa = mundo.semear_plano_padrao(empresa);
        let receita = Contas::nova(&mundo, empresa)
            .papel(PapelConta::ReceitaVendas)
            .unwrap();
        let venda = lancar_venda(&mut mundo, empresa, caixa, receita);
        Razao::estornar(&mut mundo, venda, "motivo válido, dez+", hoje()).unwrap();

        assert!(matches!(
            Razao::confirmar(&mut mundo, venda).unwrap_err(),
            ErroRazao::TransicaoInvalida { .. }
        ));
        assert!(matches!(
            Razao::liquidar(&mut mundo, venda, hoje()).unwrap_err(),
            ErroRazao::TransicaoInvalida { .. }
        ));
    }
}
