//! A porta que o Razão usa para ler e gravar, sem conhecer SQLite.
//!
//! `cardeal-storage` ainda não existe (`docs/17-roadmap.md`, Fase 0). Em vez de travar
//! `Razao` esperando por ele, o núcleo depende só desta trait — um caso do padrão de portas
//! e adaptadores do `docs/01-arquitetura-geral.md` §5. Quando `cardeal-storage` nascer, sua
//! `UnidadeDeTrabalho` (ver `docs/contratos-internos.md` §2) implementa `PortaRazao` sobre a
//! transação SQLite real; os testes deste crate já implementam `PortaRazao` em memória, e
//! continuam válidos sem alteração.

use cardeal_kernel::{Data, Id, Instante};

use crate::conta::{PapelConta, TipoConta};
use crate::erros::ErroRazao;
use crate::lancamento::Lancamento;

/// Atalho para o resultado das operações da porta.
type Resultado<T> = Result<T, ErroRazao>;

/// O suficiente sobre uma conta para `Razao::registrar` validar uma partida, sem carregar
/// o registro inteiro.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InfoConta {
    /// A empresa dona da conta.
    pub empresa: Id,
    /// O código, só para compor mensagens de erro legíveis.
    pub codigo: String,
    /// Sintética ou analítica.
    pub tipo: TipoConta,
    /// Se falsa, a conta não aceita lançamento.
    pub ativa: bool,
}

/// Tudo que o domínio do Razão precisa do mundo exterior.
///
/// Implementações mutáveis (`&mut self`) participam de uma única unidade de trabalho —
/// o adaptador real garante que as mutações só valem se a transação inteira confirmar.
///
/// Todos os métodos que devolvem `Result` falham com [`ErroRazao::FalhaDePersistencia`] em
/// erro de infraestrutura (SQLite, disco); os que devolvem `Result<Option<_>>` usam `None`
/// para "não existe", nunca para falha.
#[allow(clippy::missing_errors_doc)]
pub trait PortaRazao {
    // ── contexto ambiente da unidade de trabalho em curso ───────────────────
    // Espelha `docs/contratos-internos.md` §2 (`ContextoEscrita`/`UnidadeDeTrabalho`):
    // quem está escrevendo, de onde, e quando — para que `estornar` não precise receber
    // usuário e dispositivo como parâmetro em toda chamada.

    /// O usuário responsável pela unidade de trabalho atual.
    fn usuario(&self) -> Id;
    /// O dispositivo de onde partiu o comando.
    fn dispositivo(&self) -> Id;
    /// O instante da unidade de trabalho (do relógio, real ou controlado em teste).
    fn agora(&self) -> Instante;

    // ── contas ────────────────────────────────────────────────────────────
    /// Informações mínimas da conta, para validar uma partida. `Ok(None)` = a conta não
    /// existe; `Err` = falha de infraestrutura.
    fn info_conta(&self, conta: Id) -> Resultado<Option<InfoConta>>;
    /// Resolve o papel semântico para o `Id` da conta analítica correspondente,
    /// na empresa informada.
    fn conta_por_papel(&self, empresa: Id, papel: PapelConta) -> Resultado<Option<Id>>;
    /// Resolve um código de conta (`"1.1.01"`) para o `Id`, na empresa informada.
    fn conta_por_codigo(&self, empresa: Id, codigo: &str) -> Resultado<Option<Id>>;

    // ── período ───────────────────────────────────────────────────────────
    /// Se algum período foi fechado para esta empresa, a data até a qual (inclusive).
    /// `Ok(None)` significa que nenhum período está fechado.
    fn periodo_fechado_ate(&self, empresa: Id) -> Resultado<Option<Data>>;

    // ── numeração ─────────────────────────────────────────────────────────
    /// O próximo número sequencial de lançamento para a empresa. Cada chamada consome
    /// um número — nunca devolve o mesmo valor duas vezes.
    fn proximo_numero_lancamento(&mut self, empresa: Id) -> Resultado<u64>;

    // ── lançamentos ───────────────────────────────────────────────────────
    /// Grava um lançamento novo.
    fn inserir_lancamento(&mut self, lancamento: Lancamento) -> Resultado<()>;
    /// Busca um lançamento pelo id. `Ok(None)` = não existe.
    fn buscar_lancamento(&self, id: Id) -> Resultado<Option<Lancamento>>;
    /// Regrava um lançamento existente (usado para transições de estado e para marcar
    /// `estornado_por` no original quando ele é estornado).
    fn atualizar_lancamento(&mut self, lancamento: Lancamento) -> Resultado<()>;
}
