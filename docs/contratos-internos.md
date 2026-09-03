# Contratos internos entre crates

> **Este documento é normativo.** As assinaturas abaixo são o contrato entre os crates do
> núcleo e todos os módulos. Quem implementa um crate do núcleo implementa exatamente estas
> assinaturas; quem escreve um módulo consome exatamente estas assinaturas. Mudança aqui é
> mudança de contrato e exige atualizar todos os consumidores no mesmo PR.
>
> Conceitos e justificativas estão nos documentos 01, 04, 05, 06, 07 e 09. Aqui só as formas.

## 1. `cardeal-kernel` (implementado)

```rust
pub use cardeal_kernel::prelude::*;

// Valores
Dinheiro   // i64 centavos. ZERO, centavos(i64), reais(i64), partes(i64,u8), em_centavos(),
           // abs(), sinal(), e_zero/e_positivo/e_negativo, min/max, nao_negativo(),
           // soma_checada/subtracao_checada, de_total(Quantidade,Preco,Arredondamento),
           // aplicar(Percentual,Arredondamento), fracao(i64,i64,Arredondamento),
           // ratear(usize)->Vec<Dinheiro>, ratear_por_pesos(&[i64])->Vec<Dinheiro>,
           // formatar(), formatar_com_simbolo(), de_str(&str)
           // Operadores: + - neg += -= *i64 /i64, Sum
Quantidade // i64 escala 1e-4. ZERO, UM, unidades(i64), milesimos(i64), interna(i64),
           // unidades_internas(), e_zero/e_positiva/e_negativa/e_inteira, abs, min, max,
           // nao_negativa(), vezes(i64), converter(i64,i64), formatar(u8), de_str
Preco      // i64 escala 1e-6. ZERO, reais, centavos, milesimos, interna, unidades_internas,
           // e_zero, formatar(), formatar_com_simbolo(), de_str
Percentual // i64 escala 1e-6 ponto percentual. ZERO, CEM, pontos(i64), milesimos(i64),
           // unidades(i64), unidades_internas(), e_zero, e_fracao_valida, complemento(),
           // da_razao(i64,i64), formatar(u8), formatar_com_simbolo(), de_str
Arredondamento  // MeioAcima | MeioPar | Baixo | Cima | Truncar
Sinal           // Positivo | Negativo | Zero
Unidade         // Un Pc Cx Fd Pct Kg G Ton L Ml M M2 M3 Cm Par Duzia Hora Diaria Mes Kwh Serv
                // .codigo() .nome() .admite_fracao()

// Identidade
Id                  // UUIDv7. NULO, novo(), de_bytes([u8;16]), em_bytes(), como_bytes(),
                    // e_nulo(), de_str(&str), curto()
ChaveIdempotencia   // nova(), de_id(Id), id(), em_bytes()
Versao              // ZERO, INICIAL, nova(u64), numero(), proxima()

// Tempo
Data        // i32 dias desde 1970-01-01. EPOCA, MINIMA, MAXIMA, de_ymd(i32,u32,u32)->Resultado,
            // de_dias(i32), em_dias(), hoje(Fuso), ano(), mes(), dia(), ymd(), dia_da_semana(),
            // mais_dias(i32), dias_ate(Data), mais_meses(i32), inicio_do_mes(), fim_do_mes(),
            // inicio_do_ano(), competencia(), formatar(), formatar_curta(), formatar_iso(),
            // de_str(&str), de_str_com_referencia(&str, Data)
Hora        // de_hms(u32,u32,u32)->Resultado, em_segundos(), hora(), minuto(), segundo(),
            // formatar(), formatar_com_segundos()
Instante    // i64 micros UTC. EPOCA, agora(), de_micros(i64), em_micros(), em_segundos(),
            // data(Fuso), hora(Fuso), de_data_hora(Data,Hora,Fuso), mais_micros, mais_segundos,
            // micros_ate(Instante), formatar(Fuso), formatar_iso()
Fuso        // UTC, BRASILIA(-180), AMAZONAS(-240), ACRE(-300), NORONHA(-120), minutos(i16)
Competencia // AAAAMM. nova(i32,u32), ano(), mes(), em_aaaamm(), proxima(), anterior(),
            // primeiro_dia(), ultimo_dia(), periodo(), formatar(), formatar_extenso()
Periodo     // { de: Data, ate: Data }. novo, dia, ultimos_dias, proximos_dias, dias(),
            // contem(Data), intersecta(Periodo), dias_iter(), formatar()
DiaDaSemana // Domingo..Sabado. nome(), abreviacao(), e_fim_de_semana()

// Documentos
Cpf Cnpj Documento InscricaoEstadual Uf
// Cpf::novo(&str)->Resultado, sem_mascara(), formatar(), mascarado()
// Cnpj::novo(&str)->Resultado, sem_mascara(), formatar(), raiz(), ordem(), e_matriz()
// Documento::novo(&str)->Resultado (detecta pelo tamanho), e_fisica(), e_juridica(),
//   sem_mascara(), mascarado()
// Uf::TODAS, sigla(), codigo_ibge(), nome(), fuso(), de_sigla(&str), de_codigo_ibge(u8)
// InscricaoEstadual::nova(&str, Uf), para_nfe(), e_contribuinte()

// Erros
Resultado<T, E = Erro>
CodigoErro   // constantes por faixa; categoria(), vale_repetir()
Erro         // { codigo, mensagem, campo, detalhes }
             // novo(CodigoErro, impl Into<String>), no_campo(&str), com_detalhes(Detalhes),
             // de_dominio(&impl ErroDominio), obrigatorio(&str), nao_encontrado(&str)
Detalhes     // { resumo, causa, acoes }  nova(resumo, causa).com(AcaoSugerida)
AcaoSugerida // nova(rotulo, acao) | primaria(rotulo, acao)
trait ErroDominio: std::error::Error {
    fn codigo(&self) -> CodigoErro;
    fn campo(&self) -> Option<&str> { None }
    fn detalhes(&self) -> Option<Detalhes> { None }
}

// Texto (cardeal_kernel::texto)
sem_acentos, chave_busca, similaridade(&str,&str)->f32, contem_normalizado,
casa_por_palavras, truncar, somente_digitos, redigir_dados_pessoais
```

## 2. `cardeal-storage`

```rust
// ─── configuração e abertura ─────────────────────────────────────────────────
pub struct ConfigArmazenamento {
    pub caminho: PathBuf,          // ":memory:" para testes
    pub somente_leitura: bool,
    pub leitores: usize,           // padrão: min(4, num_cpus)
    pub janela_lote_ms: u64,       // padrão 2
    pub maximo_lote: usize,        // padrão 64
    pub chave_cripto: Option<Segredo<String>>,
}
impl ConfigArmazenamento {
    pub fn arquivo(caminho: impl Into<PathBuf>) -> Self;
    pub fn memoria() -> Self;
}

pub struct Armazenamento;
impl Armazenamento {
    pub fn abrir(cfg: ConfigArmazenamento) -> Resultado<Self>;
    pub fn escritor(&self) -> &Escritor;
    pub fn leitor(&self) -> &Leitor;
    pub fn backup(&self) -> &Backup;
    pub fn migrar(&self, conjuntos: &[ConjuntoMigracoes]) -> Resultado<RelatorioMigracao>;
    pub fn verificar(&self) -> Resultado<Veredito>;
    pub fn versao_atual(&self) -> Versao;
    pub fn fechar(self) -> Resultado<()>;
}

// ─── escrita ─────────────────────────────────────────────────────────────────
#[derive(Clone)]
pub struct ContextoEscrita {
    pub empresa: Id,
    pub usuario: Id,
    pub dispositivo: Id,
    pub sessao: Id,
    pub agora: Instante,
    pub chave: Option<ChaveIdempotencia>,
    pub correlacao: Id,
}

pub struct Confirmado<T> { pub valor: T, pub versao: Versao }

pub struct Escritor;
impl Escritor {
    /// Enfileira a unidade de trabalho, participa do group commit e devolve
    /// após o fsync do commit. É aqui que "confirmado" ganha significado.
    pub fn executar<T, F>(&self, ctx: ContextoEscrita, f: F) -> Resultado<Confirmado<T>>
    where
        F: for<'a> FnOnce(&mut UnidadeDeTrabalho<'a>) -> Resultado<T> + Send + 'static,
        T: Send + 'static;

    pub fn metricas(&self) -> MetricasEscritor;   // tamanho medio de lote, espera, fsyncs
}

pub struct UnidadeDeTrabalho<'a>;
impl<'a> UnidadeDeTrabalho<'a> {
    // Cada tarefa roda no seu próprio SAVEPOINT dentro do lote do group commit, então o que
    // se expõe é a Connection (com savepoint ativo), não uma Transaction — módulos preparam
    // e executam suas consultas por aqui e nunca abrem conexão própria.
    pub fn conexao(&self) -> &rusqlite::Connection;
    pub fn ctx(&self) -> &ContextoEscrita;
    pub fn empresa(&self) -> Id;
    pub fn usuario(&self) -> Id;
    pub fn dispositivo(&self) -> Id;
    pub fn agora(&self) -> Instante;
    pub fn hoje(&self) -> Data;                    // fuso da empresa (hoje: fixo em Brasília)

    /// Publica evento de domínio. Vai para nucleo_outbox NA MESMA TRANSAÇÃO.
    pub fn publicar<E: EventoDominio>(&mut self, evento: E) -> Resultado<()>;

    /// Registra na auditoria encadeada por hash.
    pub fn auditar(&mut self, r: RegistroAuditoria) -> Resultado<()>;

    /// Próximo valor de uma sequência por empresa (numero de lançamento, de venda, ...).
    pub fn proximo_numero(&mut self, sequencia: &str) -> Resultado<u64>;

    /// Reserva uma faixa contígua — usado na abertura de caixa para o modo autônomo.
    pub fn reservar_faixa(&mut self, sequencia: &str, tamanho: u64) -> Resultado<FaixaNumeracao>;

    /// Trava pessimista com TTL. Solta no fim do TTL ou via `Trava::soltar(&mut uow)`.
    pub fn travar(&mut self, recurso: &str, ttl_segundos: u32) -> Resultado<Trava>;
}

pub trait EventoDominio: serde::Serialize {   // serializado síncrono: sem bound Send + 'static
    /// Nome versionado: "vendas.pedido_faturado.v1"
    const TIPO: &'static str;
    fn agregado(&self) -> Option<Id> { None }
}

pub struct RegistroAuditoria {
    pub acao: String,          // "financeiro.pagar.baixar"
    pub entidade: String,      // "financeiro_parcela"
    pub entidade_id: Option<Id>,
    pub antes: Option<serde_json::Value>,
    pub depois: Option<serde_json::Value>,
}

pub struct FaixaNumeracao { pub inicio: u64, pub fim: u64 }
pub struct Trava;  // libera no Drop

// ─── leitura ─────────────────────────────────────────────────────────────────
pub struct Leitor;
impl Leitor {
    pub fn consultar<T, F>(&self, f: F) -> Resultado<T>
    where F: FnOnce(&rusqlite::Connection) -> Resultado<T>;

    /// Garante ler o próprio efeito recém-escrito ("read your writes").
    pub fn consultar_apos<T, F>(&self, minima: Versao, f: F) -> Resultado<T>
    where F: FnOnce(&rusqlite::Connection) -> Resultado<T>;
}

// ─── migrações ───────────────────────────────────────────────────────────────
pub struct Migracao {
    pub versao: u32,
    pub nome: &'static str,
    pub sql: &'static str,
    pub tipo: TipoMigracao,   // Esquema | Dados | Indice
}
pub struct ConjuntoMigracoes {
    pub modulo: &'static str,
    pub depende_de: &'static [&'static str],
    pub migracoes: &'static [Migracao],
}
pub struct RelatorioMigracao { pub aplicadas: Vec<(String, u32, u64)>, pub duracao_ms: u64 }

// ─── integridade e backup ────────────────────────────────────────────────────
pub enum Veredito { Integro, WalPendente(u64), CorrupcaoLeve(Vec<String>), CorrupcaoGrave(String) }

pub struct Backup;
impl Backup {
    pub fn snapshot(&self, destino: &Path) -> Resultado<RelatorioBackup>;
    pub fn verificar(&self, arquivo: &Path) -> Resultado<Veredito>;
    pub fn restaurar(&self, arquivo: &Path) -> Resultado<()>;
    pub fn listar(&self) -> Resultado<Vec<InfoBackup>>;
}

// ─── outbox ──────────────────────────────────────────────────────────────────
pub struct Outbox;
impl Outbox {
    pub fn pendentes(&self, limite: usize) -> Resultado<Vec<EventoPersistido>>;
    pub fn marcar_entregue(&self, ate_seq: i64) -> Resultado<()>;
}
pub struct EventoPersistido { pub seq: i64, pub tipo: String, pub empresa: Id,
                              pub agregado: Option<Id>, pub carga: Vec<u8>, pub criado_em: Instante }

// ─── segredos ────────────────────────────────────────────────────────────────
pub struct Segredo<T>(T);   // Debug imprime "[oculto]"; zeroiza no Drop
```

## 3. `cardeal-ledger`

```rust
// ─── plano de contas ─────────────────────────────────────────────────────────
pub enum Natureza { Ativo, Passivo, PatrimonioLiquido, Receita, Despesa }
impl Natureza { pub const fn devedora(self) -> bool; }

pub enum TipoConta { Sintetica, Analitica }
pub enum GrupoFluxo { Operacional, Investimento, Financiamento }

/// Papel semântico de uma conta. É o que permite o receituário do doc 05 §5 funcionar
/// sem que os módulos conheçam códigos de conta.
#[non_exhaustive]
pub enum PapelConta {
    Caixa, Bancos, Aplicacoes, ValoresEmTransito,
    ClientesAReceber, CartoesAReceber, ChequesAReceber, AdiantamentoFornecedor,
    ImpostosARecuperar,
    EstoqueMercadorias, EstoqueMateriaPrima, EstoqueEmProcesso, EstoqueAcabado,
    Fornecedores, ImpostosARecolher, SalariosAPagar, Emprestimos, AdiantamentoCliente,
    CartoesARepassar,
    Capital, LucrosAcumulados, Retiradas,
    ReceitaVendas, ReceitaServicos, ReceitaAlugueis, ReceitaHospedagem,
    DescontosConcedidos, DevolucoesVenda, ReceitaFinanceira, OutrasReceitas,
    Cmv, CustoServico, DespesaPessoal, DespesaAdministrativa, DespesaComercial,
    Ocupacao, TaxasCartao, ImpostosSobreVenda, DespesaFinanceira, Perdas, QuebraCaixa,
}

pub struct CodigoConta(String);   // "1.1.01.001" — ordenável, hierárquico

pub struct Conta {
    pub id: Id, pub empresa: Id, pub codigo: CodigoConta, pub nome: String,
    pub natureza: Natureza, pub tipo: TipoConta, pub pai: Option<Id>, pub nivel: u8,
    pub grupo_fluxo: Option<GrupoFluxo>, pub papel: Option<PapelConta>,
    pub modulo_origem: Option<String>, pub ativa: bool, pub versao: Versao,
}

/// Plano gerencial padrão do doc 05 §3.1, semeado na instalação.
pub fn plano_padrao() -> Vec<ContaSemente>;

// ─── lançamento ──────────────────────────────────────────────────────────────
pub enum EstadoLancamento { Previsto, Confirmado, Realizado, Estornado }

pub enum Contraparte { Cliente(Id), Fornecedor(Id), Funcionario(Id), Socio(Id), Outro(Id) }

pub struct Origem { pub modulo: String, pub tipo: String, pub id: Option<Id> }
impl Origem {
    pub fn modulo(m: &str) -> Self;
    pub fn tipo(self, t: &str) -> Self;
    pub fn agregado(self, id: Id) -> Self;
}

pub struct Partida {
    pub conta: Id,
    pub valor: Dinheiro,              // + débito / − crédito
    pub contraparte: Option<Contraparte>,
    pub centro_custo: Option<Id>,
    pub projeto: Option<Id>,
    pub documento: Option<String>,
    pub quantidade: Option<Quantidade>,
    pub complemento: Option<String>,
}

pub struct Lancamento {
    pub id: Id, pub empresa: Id, pub numero: u64,
    pub competencia: Data, pub vencimento: Option<Data>, pub liquidacao: Option<Data>,
    pub estado: EstadoLancamento, pub origem: Origem, pub historico: String,
    pub estorna: Option<Id>, pub estornado_por: Option<Id>,
    pub criado_em: Instante, pub criado_por: Id, pub dispositivo: Id,
    pub partidas: SmallVec<[Partida; 4]>,
}
impl Lancamento {
    pub fn soma(&self) -> Dinheiro;                 // deve ser ZERO
    pub fn total_debitos(&self) -> Dinheiro;
    pub fn total_creditos(&self) -> Dinheiro;
}

/// Só o construtor validado produz este tipo. Campo privado: impossível fabricar por fora.
pub struct LancamentoBalanceado(Lancamento);
impl LancamentoBalanceado { pub fn interno(&self) -> &Lancamento; }

pub struct ConstrutorLancamento;
impl ConstrutorLancamento {
    pub fn novo(empresa: Id, competencia: Data, historico: impl Into<String>) -> Self;
    pub fn origem(self, o: Origem) -> Self;
    pub fn estado(self, e: EstadoLancamento) -> Self;   // padrão: Realizado se houver liquidação
    pub fn vencimento(self, d: Data) -> Self;
    pub fn liquidacao(self, d: Data) -> Self;
    pub fn debitar(self, conta: Id, valor: Dinheiro) -> Self;
    pub fn creditar(self, conta: Id, valor: Dinheiro) -> Self;
    /// Variantes que ignoram a partida quando o valor é zero — evitam `if` no chamador.
    pub fn debitar_se(self, conta: Id, valor: Dinheiro) -> Self;
    pub fn creditar_se(self, conta: Id, valor: Dinheiro) -> Self;
    /// Aplicam-se à última partida adicionada.
    pub fn contraparte(self, c: Contraparte) -> Self;
    pub fn centro_custo(self, cc: Id) -> Self;
    pub fn projeto(self, p: Id) -> Self;
    pub fn documento(self, d: impl Into<String>) -> Self;
    pub fn quantidade(self, q: Quantidade) -> Self;
    pub fn complemento(self, c: impl Into<String>) -> Self;
    pub fn construir(self) -> Resultado<LancamentoBalanceado>;
}

// ─── operações ───────────────────────────────────────────────────────────────
pub struct Razao;
impl Razao {
    pub fn registrar(uow: &mut UnidadeDeTrabalho, l: LancamentoBalanceado) -> Resultado<Id>;
    pub fn estornar(uow: &mut UnidadeDeTrabalho, original: Id, motivo: &str, competencia: Data)
        -> Resultado<Id>;
    pub fn confirmar(uow: &mut UnidadeDeTrabalho, id: Id) -> Resultado<()>;
    pub fn liquidar(uow: &mut UnidadeDeTrabalho, id: Id, quando: Data) -> Resultado<()>;
    pub fn fechar_periodo(uow: &mut UnidadeDeTrabalho, ate: Data) -> Resultado<[u8; 32]>;
    pub fn reabrir_periodo(uow: &mut UnidadeDeTrabalho, ate: Data, motivo: &str) -> Resultado<()>;
}

/// Resolve PapelConta → Id, com cache. Toda escrita de módulo passa por aqui.
pub struct Contas;
impl Contas {
    pub fn carregar(c: &rusqlite::Connection, empresa: Id) -> Resultado<Self>;
    pub fn papel(&self, p: PapelConta) -> Resultado<Id>;
    pub fn caixa(&self, caixa: Id) -> Resultado<Id>;         // conta analítica do caixa
    pub fn banco(&self, conta_bancaria: Id) -> Resultado<Id>;
    pub fn por_codigo(&self, codigo: &str) -> Resultado<Id>;
}

// ─── consultas ───────────────────────────────────────────────────────────────
pub mod consultas {
    pub fn saldo_conta(c: &Connection, empresa: Id, conta: Id, ate: Data) -> Resultado<Dinheiro>;
    pub fn saldo_por_papel(c: &Connection, empresa: Id, p: PapelConta, ate: Data) -> Resultado<Dinheiro>;
    pub fn balancete(c: &Connection, empresa: Id, periodo: Periodo) -> Resultado<Vec<LinhaBalancete>>;
    pub fn razao_conta(c: &Connection, empresa: Id, conta: Id, periodo: Periodo, cursor: Option<Cursor>)
        -> Resultado<Pagina<LinhaRazao>>;
    pub fn fluxo_caixa(c: &Connection, empresa: Id, periodo: Periodo, granularidade: Granularidade)
        -> Resultado<Vec<PontoFluxo>>;
    pub fn dre(c: &Connection, empresa: Id, periodo: Periodo) -> Resultado<Dre>;
    pub fn lancamentos_da_origem(c: &Connection, origem: &Origem) -> Resultado<Vec<Lancamento>>;
    pub fn provar(c: &Connection, empresa: Id) -> Resultado<ProvaRazao>;
}

pub struct PontoFluxo {
    pub dia: Data,
    pub realizado: Dinheiro,
    pub comprometido: Dinheiro,
    pub previsto: Dinheiro,
    pub saldo_acumulado: Dinheiro,
}
pub enum Granularidade { Dia, Semana, Mes, Trimestre }
pub struct ProvaRazao { pub balanceado: bool, pub diferenca: Dinheiro,
                        pub lancamentos_suspeitos: Vec<Id>, pub partidas_conferidas: u64 }

// ─── erros ───────────────────────────────────────────────────────────────────
pub enum ErroRazao {
    Desbalanceado { diferenca: Dinheiro, partidas: usize },
    PartidasInsuficientes,
    ContaNaoEncontrada(Id),
    ContaSintetica { conta: Id, codigo: String },
    ContaInativa(Id),
    PapelNaoMapeado(PapelConta),
    PeriodoFechado { ate: Data },
    ValorZerado,
    JaEstornado(Id),
    LancamentoNaoEncontrado(Id),
    EmpresaDivergente,
}
// implementa ErroDominio
```

## 4. `cardeal-modkit`

```rust
pub struct IdModulo(&'static str);

pub struct Submodulo {
    pub id: &'static str, pub nome: &'static str,
    pub essencial: bool, pub depende_de: &'static [&'static str],
}

pub enum Risco { Baixo, Medio, Alto, Critico }

pub struct Permissao {
    pub chave: &'static str, pub descricao: &'static str,
    pub risco: Risco, pub requer_submodulo: Option<&'static str>,
}

pub struct EntradaMenu {
    pub id: &'static str, pub rotulo: &'static str, pub icone: Icone,
    pub peso: u16,                      // financeiro.pulso = 0
    pub permissao: &'static str,
    pub requer_submodulo: Option<&'static str>,
    pub pai: Option<&'static str>,
}

pub struct ContaPadrao { pub papel: PapelConta, pub obrigatoria: bool }

pub struct Manifesto {
    pub id: IdModulo, pub nome: &'static str, pub versao: (u16, u16, u16),
    pub descricao: &'static str, pub icone: Icone,
    pub depende_de: &'static [IdModulo],
    pub melhora_com: &'static [IdModulo],
    pub conflita_com: &'static [IdModulo],
    pub submodulos: &'static [Submodulo],
    pub permissoes: &'static [Permissao],
    pub menu: &'static [EntradaMenu],
    pub contas_requeridas: &'static [ContaPadrao],
    pub eventos_publicados: &'static [&'static str],
    pub eventos_assinados: &'static [&'static str],
}

pub trait Modulo: Send + Sync + 'static {
    fn manifesto(&self) -> &'static Manifesto;
    fn migracoes(&self) -> ConjuntoMigracoes;
    fn registrar(&self, r: &mut Registro) -> Resultado<()>;
    fn ao_ativar(&self, ctx: &ContextoAtivacao) -> Resultado<()> { Ok(()) }
    fn diagnostico(&self, ctx: &Ctx) -> Vec<Checagem> { Vec::new() }
}

/// Contexto passado a comandos e consultas.
pub struct Ctx<'a> {
    pub empresa: Id, pub usuario: Id, pub dispositivo: Id, pub sessao: Id,
    pub agora: Instante, pub fuso: Fuso, pub correlacao: Id,
}
impl<'a> Ctx<'a> {
    pub fn hoje(&self) -> Data;
    pub fn contas(&self) -> &Contas;
    pub fn ativo(&self, modulo: &str) -> bool;
    pub fn submodulo_ativo(&self, modulo: &str, submodulo: &str) -> bool;
    pub fn config<T: DeserializeOwned>(&self, chave: &str) -> Resultado<Option<T>>;
    pub fn limite(&self, chave: &str) -> Option<ValorLimite>;
    pub fn porta<P: ?Sized + 'static>(&self) -> Option<&P>;   // portas registradas
}

pub trait Comando: serde::de::DeserializeOwned + Send + 'static {
    type Saida: serde::Serialize + Send + 'static;
    const PERMISSAO: &'static str;
    const RISCO: Risco = Risco::Medio;
    const AUDITA: bool = false;
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida>;
}

pub trait Consulta: serde::de::DeserializeOwned + Send + 'static {
    type Saida: serde::Serialize + Send + 'static;
    const PERMISSAO: &'static str;
    fn executar(self, ctx: &Ctx, c: &rusqlite::Connection) -> Resultado<Self::Saida>;
}

pub struct Registro;
impl Registro {
    pub fn comando<C: Comando>(&mut self, nome: &'static str) -> &mut Self;
    pub fn consulta<Q: Consulta>(&mut self, nome: &'static str) -> &mut Self;
    pub fn assinar<E: DeserializeOwned>(&mut self, evento: &'static str,
        f: impl Fn(&Ctx, &mut UnidadeDeTrabalho, E) -> Resultado<()> + Send + Sync + 'static) -> &mut Self;
    pub fn tarefa(&mut self, t: Tarefa) -> &mut Self;
    pub fn fornecer_porta<P: ?Sized + 'static>(&mut self, p: Arc<P>) -> &mut Self;
    pub fn item_pulso(&mut self, f: FonteItemPulso) -> &mut Self;   // "exige ação hoje"
}

pub struct Tarefa { pub id: &'static str, pub quando: Agendamento, pub acao: ... }
pub enum Agendamento { Intervalo(Duration), Diario { hora: Hora }, AoIniciar, AoOcioso }

/// Cursor opaco de paginação keyset.
pub struct Cursor(Vec<u8>);
pub struct Pagina<T> { pub itens: Vec<T>, pub proximo: Option<Cursor>,
                       pub total_aproximado: Option<u64>, pub versao: Versao }

pub struct Perfil {
    pub id: &'static str, pub nome: &'static str, pub descricao: &'static str,
    pub modulos: &'static [&'static str],
    pub submodulos: &'static [(&'static str, &'static [&'static str])],
    pub configuracao: &'static [(&'static str, &'static str)],
}
pub fn perfis() -> &'static [Perfil];   // mei, comercio, posto, industria, hotel,
                                        // locadora, assistencia, servicos

pub enum Icone { Pulso, Dinheiro, Carrinho, Caixa, Pessoas, Estoque, Nota, Grafico,
                 Agenda, Ferramenta, Chave, Config, Banco, Conciliar, Alerta, /* ... */ }
```

## 5. `cardeal-auth`

```rust
// ─── senha e bloqueio (implementado) ─────────────────────────────────────────
pub fn hash_senha(senha: &str) -> Result<HashDeSenha, ErroAuth>;
pub fn verificar_senha(senha: &str, hash: &HashDeSenha) -> bool;
pub struct HashDeSenha;   // Debug = "[oculto]"; de_phc(String), como_phc() -> &str
pub struct PoliticaSenha { pub minimo_caracteres: u8 }   // PADRAO; validar(&str)
pub struct Bloqueio { pub tentativas: u32, pub bloqueado_ate: Option<Instante> }
// NOVO, esta_bloqueado, segundos_restantes, registrar_falha, registrar_sucesso,
// desbloquear_pelo_administrador

// ─── escopo (ABAC — implementado) ────────────────────────────────────────────
pub struct Escopo {
    pub empresa: Id,
    pub filial: Option<Id>,
    pub caixa: Option<Id>,
    pub centro_custo: Option<Id>,
}
impl Escopo {
    pub const fn empresa_inteira(empresa: Id) -> Self;
    pub const fn com_filial(self, Id) -> Self;
    pub const fn com_caixa(self, Id) -> Self;
    pub const fn com_centro_custo(self, Id) -> Self;
    /// `self` (autorização) abrange `recurso`: mesma empresa e, em cada dimensão,
    /// `None` = sem restrição, `Some` = igualdade exata.
    pub fn abrange(&self, recurso: &Self) -> bool;
}

// ─── usuário (domínio — implementado) ────────────────────────────────────────
pub struct Usuario {
    pub id: Id, pub login: String, pub nome: String, pub email: Option<String>,
    pub hash_senha: HashDeSenha, pub senha_trocada_em: Instante,
    pub exige_troca_senha: bool, pub mfa_habilitado: bool, pub ativo: bool,
    pub bloqueio: Bloqueio, pub versao: Versao,
}
impl Usuario {
    pub fn novo(login, nome, senha_inicial: &str, PoliticaSenha, agora: Instante)
        -> Result<Self, ErroAuth>;
    /// Sucesso zera o bloqueio; falha registra e aplica bloqueio progressivo.
    /// Erro genérico (CredencialInvalida) para conta inexistente/inativa/senha errada.
    pub fn autenticar(&mut self, senha: &str, agora: Instante) -> Result<(), ErroAuth>;
    pub fn trocar_senha(&mut self, atual, nova, PoliticaSenha, agora) -> Result<(), ErroAuth>;
    pub fn redefinir_senha(&mut self, nova, PoliticaSenha, agora) -> Result<(), ErroAuth>;
    pub fn desbloquear(&mut self);
    pub fn desativar(&mut self);
    pub fn reativar(&mut self);
}

// ─── papéis e limites (domínio — implementado) ───────────────────────────────
pub enum ValorLimite { Dinheiro(Dinheiro), Percentual(Percentual), Contagem(u32),
                       Dias(u32), Ilimitado }
// comporta_dinheiro/percentual/contagem/dias(valor) -> bool; mais_permissivo(self, Self)

pub struct Papel {
    pub id: Id, pub empresa: Option<Id>, pub nome: String, pub descricao: String,
    pub sistema: bool, pub permissoes: BTreeSet<String>,
    pub limites: BTreeMap<String, ValorLimite>, pub versao: Versao,
}
impl Papel {
    pub fn novo(empresa: Id, nome) -> Self;
    pub fn com_permissao(self, chave) -> Self;
    pub fn com_limite(self, chave, ValorLimite) -> Self;
    pub fn concede(&self, permissao: &str) -> bool;
    pub fn garantir_editavel(&self) -> Result<(), ErroAuth>;   // Err se sistema
    pub fn duplicar_para(&self, empresa: Id, nome) -> Self;
}

pub enum PapelDeFabrica { Administrador, Gerente, Financeiro, OperadorCaixa,
    Vendedor, Estoquista, Comprador, Contador, Auditor }   // docs/08 §3.2
impl PapelDeFabrica {
    pub const TODOS: [Self; 9];
    pub const fn id(self) -> &'static str;
    pub const fn nome(self) -> &'static str;
    pub const fn descricao(self) -> &'static str;
    pub const fn politica(self) -> PoliticaPapel;
    /// Expande a política contra o catálogo real de chaves (de cardeal-modkit) →
    /// Papel global, sistema = true.
    pub fn materializar<'a>(self, catalogo: impl IntoIterator<Item = &'a str>) -> Papel;
}
pub struct PoliticaPapel {   // regra declarativa: nega vence, depois tudo/módulo/prefixo/sufixo
    pub tudo: bool,
    pub nega_prefixo: &'static [&'static str],
    pub modulos: &'static [&'static str],
    pub concede_prefixo: &'static [&'static str],
    pub concede_sufixo: &'static [&'static str],
}

// ─── sessão e autorização (domínio — implementado) ───────────────────────────
pub struct AutorizacoesEfetivas;   // conjunto resolvido de permissões + limites
impl AutorizacoesEfetivas {
    pub fn consolidar<'a>(papeis: impl IntoIterator<Item = &'a Papel>) -> Self;
    pub fn restringir_a<'a>(self, visiveis: impl IntoIterator<Item = &'a str>) -> Self;
    pub fn concede(&self, permissao: &str) -> bool;
    pub fn limite(&self, chave: &str) -> Option<ValorLimite>;
}

pub const DURACAO_PADRAO_SEGUNDOS: i64 = 15 * 60;
pub struct EmissaoSessao {
    pub usuario: Id, pub dispositivo: Id, pub escopo: Escopo,
    pub autorizacoes: AutorizacoesEfetivas, pub emitida_em: Instante,
    pub duracao_segundos: i64,
}
impl EmissaoSessao { pub fn padrao(usuario, dispositivo, escopo, autorizacoes, emitida_em) -> Self; }

pub struct Sessao {
    pub id: Id, pub usuario: Id, pub dispositivo: Id, pub escopo: Escopo,
    pub emitida_em: Instante, pub expira_em: Instante, pub encerrada_em: Option<Instante>,
}
impl Sessao {
    pub fn abrir(EmissaoSessao) -> Self;
    pub fn empresa(&self) -> Id;
    pub fn esta_valida(&self, agora: Instante) -> bool;
    pub fn renovar(&mut self, agora: Instante, duracao_segundos: i64) -> Result<(), ErroAuth>;
    pub fn encerrar(&mut self, agora: Instante);
    pub fn autorizacoes(&self) -> &AutorizacoesEfetivas;
    pub fn limite(&self, chave: &str) -> Option<ValorLimite>;
}

/// O único ponto de verificação (docs/08 §3.5). Passa se a sessão concede a
/// permissão E o escopo dela abrange o recurso. Sessão encerrada é recusada aqui
/// também (defesa em profundidade); a expiração é da borda de transporte.
pub fn autorizar(sessao: &Sessao, permissao: &str, recurso: &Escopo) -> Result<(), ErroAuth>;

// ─── erros ───────────────────────────────────────────────────────────────────
pub enum ErroAuth {
    SenhaCurta { minimo: u8 }, SenhaComum, FalhaDeHash,
    CredencialInvalida, ContaBloqueada { ate: Instante },
    ForaDoEscopo, SemPermissao { permissao: String }, SessaoEncerrada,
    PapelDoSistema, SenhaAtualIncorreta,
}
// implementa ErroDominio
```

**Nota de dependência:** `cardeal-auth` **não** depende de `cardeal-modkit` — é o contrário
(o despacho de `cardeal-modkit` chama [`autorizar`]). Por isso a expansão "papel de fábrica
→ permissões" recebe o catálogo de chaves por parâmetro, e a montagem de
`AutorizacoesEfetivas` a partir do `ConjuntoEfetivo` é feita na camada que tem os dois.

## 6. `cardeal-ui`

```rust
pub mod tokens {
    pub struct Paleta { /* campos com egui::Color32 */ }
    impl Paleta { pub fn clara() -> Self; pub fn escura() -> Self; }
    pub const RUBRO_500: egui::Color32;   // #EF443B
    pub mod espaco { pub const XS: f32 = 4.0; /* ... */ }
    pub mod raio   { pub const CAMPO: f32 = 4.0; /* ... */ }
}

pub struct Tema { pub paleta: Paleta, pub escala: f32, pub densidade: Densidade }
impl Tema { pub fn aplicar(&self, ctx: &egui::Context); }

// Componentes — todos implementam egui::Widget
pub struct Botao;         // primario, secundario, fantasma, destrutivo; .atalho(Tecla)
pub struct CampoMoeda;    // digitação da direita para a esquerda
pub struct CampoQuantidade;
pub struct CampoPercentual;
pub struct CampoData;     // aceita "hoje", "+7", "fim do mes", "1003"
pub struct CampoDocumento;
pub struct Busca;
pub struct Valor;         // exibição de Dinheiro com sinal, cor e tabular
pub struct Badge;
pub struct Grade;         // virtualizada
pub struct Gaveta;
pub struct Sidebar;
pub struct PaletaComandos;
pub struct LinhaDoTempo;  // o Rio do Caixa
pub struct FaixaEstado;
pub struct DialogoConflito;
pub struct Vazio;         // estado vazio com ilustração e ação
```

## 7. Regras de uso

1. Módulo **nunca** abre conexão. Recebe `&mut UnidadeDeTrabalho` (escrita) ou
   `&rusqlite::Connection` (leitura).
2. Módulo **nunca** lê tabela de outro módulo. Prefixo de tabela = id do módulo.
3. Todo efeito financeiro passa por `ConstrutorLancamento` + `Razao::registrar`.
4. Nenhuma conta é referenciada por código literal: use `ctx.contas().papel(PapelConta::X)`.
5. Todo comando declara `PERMISSAO`. Sem isso não compila.
6. Toda consulta que retorna lista usa `Pagina<T>` com `Cursor`.
7. Erros de módulo implementam `ErroDominio`.
8. Nada de `f64` para dinheiro, quantidade, preço ou percentual.
