//! Todo o SQL do módulo financeiro (`docs/15-convencoes-codigo.md` §5): um único lugar com
//! strings SQL, sempre colunas explícitas, sempre parâmetros nomeados ou posicionais fixos.
//!
//! [`RepositorioFinanceiro`] grava e lê `financeiro_titulo`/`financeiro_parcela`/
//! `financeiro_baixa` sobre a [`UnidadeDeTrabalho`](cardeal_storage::UnidadeDeTrabalho) do
//! escritor único — mesmo padrão do `RepositorioRazao`.

use cardeal_kernel::{
    CodigoErro, Data, Dinheiro, Erro, Id, Instante, Percentual, Resultado, Versao,
};
use cardeal_ledger::Contraparte;
use cardeal_storage::UnidadeDeTrabalho;
use rusqlite::{params, Connection, OptionalExtension};

use crate::caixa::{Caixa, EstadoSessao, MovimentoCaixa, SessaoCaixa, TipoMovimento};
use crate::categoria::CategoriaFinanceira;
use crate::recorrencia::{Periodicidade, Recorrencia, TipoValor};
use crate::titulo::{
    EspecieTitulo, EstadoParcela, FormaCobranca, Parcela, PoliticaJuros, Titulo, TituloComParcelas,
};

#[allow(clippy::needless_pass_by_value)] // usado como `.map_err(persist)`
pub(crate) fn persist(e: rusqlite::Error) -> Erro {
    Erro::novo(CodigoErro::FALHA_INTERNA, format!("financeiro/SQL: {e}"))
}

pub(crate) fn blob(id: Id) -> Vec<u8> {
    id.em_bytes().to_vec()
}

fn blob_opt(id: Option<Id>) -> Option<Vec<u8>> {
    id.map(blob)
}

pub(crate) fn id_de(bytes: Vec<u8>) -> Id {
    <[u8; 16]>::try_from(bytes).map_or(Id::NULO, Id::de_bytes)
}

fn dias(d: Data) -> i64 {
    i64::from(d.em_dias())
}

pub(crate) fn data_de(dias: i64) -> Data {
    Data::de_dias(i32::try_from(dias).unwrap_or(0))
}

fn versao_de(n: i64) -> Versao {
    Versao::nova(u64::try_from(n).unwrap_or(1))
}

fn versao_i64(v: Versao) -> i64 {
    i64::try_from(v.numero()).unwrap_or(i64::MAX)
}

pub(crate) fn especie_txt(e: EspecieTitulo) -> &'static str {
    match e {
        EspecieTitulo::Receber => "Receber",
        EspecieTitulo::Pagar => "Pagar",
    }
}

pub(crate) fn especie_de(s: &str) -> EspecieTitulo {
    match s {
        "Pagar" => EspecieTitulo::Pagar,
        _ => EspecieTitulo::Receber,
    }
}

pub(crate) fn especie_opt_txt(e: Option<EspecieTitulo>) -> Option<&'static str> {
    e.map(especie_txt)
}

pub(crate) fn especie_opt_de(s: Option<String>) -> Option<EspecieTitulo> {
    s.map(|s| especie_de(&s))
}

fn tipo_valor_txt(t: TipoValor) -> &'static str {
    match t {
        TipoValor::Fixo => "Fixo",
        TipoValor::Indexado => "Indexado",
        TipoValor::Variavel => "Variavel",
    }
}

fn tipo_valor_de(s: &str) -> TipoValor {
    match s {
        "Indexado" => TipoValor::Indexado,
        "Variavel" => TipoValor::Variavel,
        _ => TipoValor::Fixo,
    }
}

fn periodicidade_txt(p: Periodicidade) -> &'static str {
    match p {
        Periodicidade::Mensal => "Mensal",
        Periodicidade::Semanal => "Semanal",
        Periodicidade::Anual => "Anual",
        Periodicidade::Personalizada => "Personalizada",
    }
}

fn periodicidade_de(s: &str) -> Periodicidade {
    match s {
        "Semanal" => Periodicidade::Semanal,
        "Anual" => Periodicidade::Anual,
        "Personalizada" => Periodicidade::Personalizada,
        _ => Periodicidade::Mensal,
    }
}

fn forma_txt(f: FormaCobranca) -> &'static str {
    match f {
        FormaCobranca::Boleto => "Boleto",
        FormaCobranca::Pix => "Pix",
        FormaCobranca::Carteira => "Carteira",
        FormaCobranca::DebitoAutomatico => "DebitoAutomatico",
        FormaCobranca::Cartao => "Cartao",
    }
}

fn forma_de(s: &str) -> FormaCobranca {
    match s {
        "Boleto" => FormaCobranca::Boleto,
        "Pix" => FormaCobranca::Pix,
        "DebitoAutomatico" => FormaCobranca::DebitoAutomatico,
        "Cartao" => FormaCobranca::Cartao,
        _ => FormaCobranca::Carteira,
    }
}

fn estado_txt(e: EstadoParcela) -> &'static str {
    match e {
        EstadoParcela::Aberta => "Aberta",
        EstadoParcela::Parcial => "Parcial",
        EstadoParcela::Quitada => "Quitada",
        EstadoParcela::Cancelada => "Cancelada",
        EstadoParcela::Renegociada => "Renegociada",
    }
}

pub(crate) fn estado_de(s: &str) -> EstadoParcela {
    match s {
        "Parcial" => EstadoParcela::Parcial,
        "Quitada" => EstadoParcela::Quitada,
        "Cancelada" => EstadoParcela::Cancelada,
        "Renegociada" => EstadoParcela::Renegociada,
        _ => EstadoParcela::Aberta,
    }
}

fn juros_txt(p: PoliticaJuros) -> &'static str {
    match p {
        PoliticaJuros::Nenhum => "Nenhum",
        PoliticaJuros::SimplesDiario => "SimplesDiario",
        PoliticaJuros::SimplesMensal => "SimplesMensal",
    }
}

fn juros_de(s: &str) -> PoliticaJuros {
    match s {
        "SimplesDiario" => PoliticaJuros::SimplesDiario,
        "SimplesMensal" => PoliticaJuros::SimplesMensal,
        _ => PoliticaJuros::Nenhum,
    }
}

fn contraparte_split(c: Contraparte) -> (&'static str, Vec<u8>) {
    let t = match c {
        Contraparte::Cliente(_) => "Cliente",
        Contraparte::Fornecedor(_) => "Fornecedor",
        Contraparte::Funcionario(_) => "Funcionario",
        Contraparte::Socio(_) => "Socio",
        Contraparte::Outro(_) => "Outro",
    };
    (t, blob(c.id()))
}

pub(crate) fn contraparte_join(tipo: &str, id: Vec<u8>) -> Contraparte {
    let id = id_de(id);
    match tipo {
        "Fornecedor" => Contraparte::Fornecedor(id),
        "Funcionario" => Contraparte::Funcionario(id),
        "Socio" => Contraparte::Socio(id),
        "Outro" => Contraparte::Outro(id),
        _ => Contraparte::Cliente(id),
    }
}

fn estado_sessao_txt(e: EstadoSessao) -> &'static str {
    match e {
        EstadoSessao::Aberta => "Aberta",
        EstadoSessao::Fechada => "Fechada",
        EstadoSessao::Auditada => "Auditada",
    }
}

fn estado_sessao_de(s: &str) -> EstadoSessao {
    match s {
        "Fechada" => EstadoSessao::Fechada,
        "Auditada" => EstadoSessao::Auditada,
        _ => EstadoSessao::Aberta,
    }
}

fn tipo_movimento_txt(t: TipoMovimento) -> &'static str {
    match t {
        TipoMovimento::Suprimento => "Suprimento",
        TipoMovimento::Sangria => "Sangria",
        TipoMovimento::Venda => "Venda",
        TipoMovimento::Recebimento => "Recebimento",
        TipoMovimento::Pagamento => "Pagamento",
        TipoMovimento::QuebraCaixa => "QuebraCaixa",
    }
}

fn tipo_movimento_de(s: &str) -> TipoMovimento {
    match s {
        "Sangria" => TipoMovimento::Sangria,
        "Venda" => TipoMovimento::Venda,
        "Recebimento" => TipoMovimento::Recebimento,
        "Pagamento" => TipoMovimento::Pagamento,
        "QuebraCaixa" => TipoMovimento::QuebraCaixa,
        _ => TipoMovimento::Suprimento,
    }
}

/// O que uma baixa grava em `financeiro_baixa`.
#[derive(Debug, Clone, Copy)]
pub struct BaixaGravada {
    /// Identidade da baixa.
    pub id: Id,
    /// A empresa.
    pub empresa: Id,
    /// A parcela baixada.
    pub parcela: Id,
    /// A data da baixa.
    pub data: Data,
    /// O dinheiro que efetivamente entrou.
    pub valor_recebido: Dinheiro,
    /// A parte que abateu o principal.
    pub principal: Dinheiro,
    /// Juros cobrados.
    pub juros: Dinheiro,
    /// Multa cobrada.
    pub multa: Dinheiro,
    /// Desconto concedido.
    pub desconto: Dinheiro,
    /// O lançamento `Realizado` gerado.
    pub lancamento: Id,
    /// Quando esta baixa foi estornada, se foi.
    pub estornada_em: Option<Instante>,
}

/// Grava e lê os títulos, parcelas e baixas do financeiro.
pub struct RepositorioFinanceiro<'a, 'b> {
    uow: &'a mut UnidadeDeTrabalho<'b>,
}

impl<'a, 'b> RepositorioFinanceiro<'a, 'b> {
    /// Cria o repositório sobre a unidade de trabalho corrente.
    pub fn novo(uow: &'a mut UnidadeDeTrabalho<'b>) -> Self {
        Self { uow }
    }

    fn conn(&self) -> &Connection {
        self.uow.conexao()
    }

    /// Grava o título e todas as suas parcelas.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn inserir_titulo(&mut self, tcp: &TituloComParcelas) -> Resultado<()> {
        let t = &tcp.titulo;
        let (cp_tipo, cp_id) = contraparte_split(t.contraparte);
        self.conn()
            .execute(
                "INSERT INTO financeiro_titulo
                   (id, empresa, especie, contraparte_tipo, contraparte_id, origem_modulo,
                    origem_id, emissao, valor_original, forma_cobranca, centro_custo, categoria,
                    observacao, cancelado_em, versao, criado_em, criado_por)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,NULL,?14,?15,?16)",
                params![
                    blob(t.id),
                    blob(t.empresa),
                    especie_txt(t.especie),
                    cp_tipo,
                    cp_id,
                    t.origem_modulo,
                    blob_opt(t.origem_id),
                    dias(t.emissao),
                    t.valor_original.em_centavos(),
                    forma_txt(t.forma_cobranca),
                    blob_opt(t.centro_custo),
                    blob_opt(t.categoria),
                    t.observacao,
                    versao_i64(t.versao),
                    self.uow.agora().em_micros(),
                    blob(self.uow.usuario()),
                ],
            )
            .map_err(persist)?;

        for p in &tcp.parcelas {
            self.inserir_parcela(p)?;
        }
        Ok(())
    }

    fn inserir_parcela(&self, p: &Parcela) -> Resultado<()> {
        self.conn()
            .execute(
                "INSERT INTO financeiro_parcela
                   (id, empresa, titulo, numero, vencimento, valor, estado, valor_baixado,
                    lancamento, nosso_numero, politica_juros, taxa_juros, multa, desconto_ate,
                    desconto_valor, versao)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)",
                params![
                    blob(p.id),
                    blob(p.empresa),
                    blob(p.titulo),
                    i64::from(p.numero),
                    dias(p.vencimento),
                    p.valor.em_centavos(),
                    estado_txt(p.estado),
                    p.valor_baixado.em_centavos(),
                    blob_opt(p.lancamento),
                    p.nosso_numero,
                    juros_txt(p.politica_juros),
                    p.taxa_juros.map(Percentual::unidades_internas),
                    p.multa.map(Percentual::unidades_internas),
                    p.desconto_ate_data.map(dias),
                    p.desconto_valor.map(Dinheiro::em_centavos),
                    versao_i64(p.versao),
                ],
            )
            .map_err(persist)?;
        Ok(())
    }

    /// Regrava o estado de uma parcela (baixa aplicada, lançamento vinculado, versão).
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn atualizar_parcela(&mut self, p: &Parcela) -> Resultado<()> {
        self.conn()
            .execute(
                "UPDATE financeiro_parcela
                 SET estado = ?2, valor_baixado = ?3, lancamento = ?4, versao = ?5
                 WHERE id = ?1",
                params![
                    blob(p.id),
                    estado_txt(p.estado),
                    p.valor_baixado.em_centavos(),
                    blob_opt(p.lancamento),
                    versao_i64(p.versao),
                ],
            )
            .map_err(persist)?;
        Ok(())
    }

    /// Grava uma baixa.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn inserir_baixa(&mut self, b: &BaixaGravada) -> Resultado<()> {
        self.conn()
            .execute(
                "INSERT INTO financeiro_baixa
                   (id, empresa, parcela, data, valor_recebido, principal, juros, multa,
                    desconto, lancamento, estornada_em, criado_em, criado_por)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,NULL,?11,?12)",
                params![
                    blob(b.id),
                    blob(b.empresa),
                    blob(b.parcela),
                    dias(b.data),
                    b.valor_recebido.em_centavos(),
                    b.principal.em_centavos(),
                    b.juros.em_centavos(),
                    b.multa.em_centavos(),
                    b.desconto.em_centavos(),
                    blob(b.lancamento),
                    self.uow.agora().em_micros(),
                    blob(self.uow.usuario()),
                ],
            )
            .map_err(persist)?;
        Ok(())
    }

    /// Busca uma parcela pelo id. `Ok(None)` = não existe.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn buscar_parcela(&self, id: Id) -> Resultado<Option<Parcela>> {
        self.conn()
            .query_row(
                "SELECT id, empresa, titulo, numero, vencimento, valor, estado, valor_baixado,
                        lancamento, nosso_numero, politica_juros, taxa_juros, multa,
                        desconto_ate, desconto_valor, versao
                 FROM financeiro_parcela WHERE id = ?1",
                [blob(id)],
                parcela_de_linha,
            )
            .optional()
            .map_err(persist)
    }

    /// Busca um título pelo id. `Ok(None)` = não existe.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn buscar_titulo(&self, id: Id) -> Resultado<Option<Titulo>> {
        self.conn()
            .query_row(
                "SELECT id, empresa, especie, contraparte_tipo, contraparte_id, origem_modulo,
                        origem_id, emissao, valor_original, forma_cobranca, centro_custo,
                        categoria, observacao, cancelado_em, versao
                 FROM financeiro_titulo WHERE id = ?1",
                [blob(id)],
                titulo_de_linha,
            )
            .optional()
            .map_err(persist)
    }

    /// Todas as parcelas de um título, em ordem.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn parcelas_do_titulo(&self, titulo: Id) -> Resultado<Vec<Parcela>> {
        let mut stmt = self
            .conn()
            .prepare(
                "SELECT id, empresa, titulo, numero, vencimento, valor, estado, valor_baixado,
                        lancamento, nosso_numero, politica_juros, taxa_juros, multa,
                        desconto_ate, desconto_valor, versao
                 FROM financeiro_parcela WHERE titulo = ?1 ORDER BY numero",
            )
            .map_err(persist)?;
        let linhas = stmt
            .query_map([blob(titulo)], parcela_de_linha)
            .map_err(persist)?;
        linhas
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(persist)
    }

    /// Busca uma baixa pelo id. `Ok(None)` = não existe.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn buscar_baixa(&self, id: Id) -> Resultado<Option<BaixaGravada>> {
        self.conn()
            .query_row(
                "SELECT id, empresa, parcela, data, valor_recebido, principal, juros, multa,
                        desconto, lancamento, estornada_em
                 FROM financeiro_baixa WHERE id = ?1",
                [blob(id)],
                baixa_de_linha,
            )
            .optional()
            .map_err(persist)
    }

    /// Marca uma baixa como estornada — idempotência: `EstornarBaixa` recusa se já estiver
    /// marcada (ver `ErroFinanceiro::BaixaJaEstornada`).
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn marcar_baixa_estornada(&mut self, id: Id) -> Resultado<()> {
        let quando = self.uow.agora().em_micros();
        self.conn()
            .execute(
                "UPDATE financeiro_baixa SET estornada_em = ?2 WHERE id = ?1",
                params![blob(id), quando],
            )
            .map_err(persist)?;
        Ok(())
    }

    /// As parcelas em aberto (`Aberta`/`Parcial`) de uma espécie, por vencimento — o que
    /// sustenta as telas de Contas a Receber/Pagar e o Pulso (`docs/modulos/financeiro.md`
    /// §10). Mesmo padrão de `RepositorioAuth`: a leitura é uma função livre sobre
    /// `&Connection` (`consultas::titulos_em_aberto`) para servir tanto esta instância
    /// (dentro de uma transação de escrita) quanto uma `Consulta` (sobre o `Leitor`).
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn titulos_em_aberto(
        &self,
        empresa: Id,
        especie: EspecieTitulo,
    ) -> Resultado<Vec<crate::consultas::ItemTituloEmAberto>> {
        crate::consultas::titulos_em_aberto(self.conn(), empresa, especie)
    }

    /// Grava uma categoria financeira nova.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn inserir_categoria(&mut self, c: &CategoriaFinanceira) -> Resultado<()> {
        self.conn()
            .execute(
                "INSERT INTO financeiro_categoria (id, empresa, nome, especie, ativa)
                 VALUES (?1,?2,?3,?4,?5)",
                params![
                    blob(c.id),
                    blob(c.empresa),
                    c.nome,
                    especie_opt_txt(c.especie),
                    i64::from(c.ativa),
                ],
            )
            .map_err(persist)?;
        Ok(())
    }

    /// As categorias ativas de uma empresa, por nome — o que alimenta o seletor de
    /// categoria na tela de lançamento.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn categorias_ativas(&self, empresa: Id) -> Resultado<Vec<CategoriaFinanceira>> {
        crate::consultas::categorias_ativas(self.conn(), empresa)
    }

    /// Grava uma regra de recorrência nova.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn inserir_recorrencia(&mut self, r: &Recorrencia) -> Resultado<()> {
        let (cp_tipo, cp_id) = contraparte_split(r.contraparte);
        self.conn()
            .execute(
                "INSERT INTO financeiro_recorrencia
                   (id, empresa, descricao, especie, contraparte_tipo, contraparte_id,
                    tipo_valor, valor_fixo, indice, media_ultimos_n, periodicidade,
                    dia_referencia, expressao_cron, inicio, fim, conta_contrapartida,
                    centro_custo, categoria, antecedencia_geracao_dias, ativa, versao)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21)",
                params![
                    blob(r.id),
                    blob(r.empresa),
                    r.descricao,
                    especie_txt(r.especie),
                    cp_tipo,
                    cp_id,
                    tipo_valor_txt(r.tipo_valor),
                    r.valor_fixo.map(Dinheiro::em_centavos),
                    r.indice,
                    r.media_ultimos_n.map(i64::from),
                    periodicidade_txt(r.periodicidade),
                    r.dia_referencia.map(i64::from),
                    r.expressao_cron,
                    dias(r.inicio),
                    r.fim.map(dias),
                    blob(r.conta_contrapartida),
                    blob_opt(r.centro_custo),
                    blob_opt(r.categoria),
                    i64::from(r.antecedencia_geracao_dias),
                    i64::from(r.ativa),
                    versao_i64(r.versao),
                ],
            )
            .map_err(persist)?;
        Ok(())
    }

    /// Busca uma recorrência pelo id. `Ok(None)` = não existe.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn buscar_recorrencia(&self, id: Id) -> Resultado<Option<Recorrencia>> {
        self.conn()
            .query_row(
                "SELECT id, empresa, descricao, especie, contraparte_tipo, contraparte_id,
                        tipo_valor, valor_fixo, indice, media_ultimos_n, periodicidade,
                        dia_referencia, expressao_cron, inicio, fim, conta_contrapartida,
                        centro_custo, categoria, antecedencia_geracao_dias, ativa, versao
                 FROM financeiro_recorrencia WHERE id = ?1",
                [blob(id)],
                recorrencia_de_linha,
            )
            .optional()
            .map_err(persist)
    }

    /// As recorrências ativas de uma empresa — a varredura que
    /// `materializar_recorrencias_pendentes` faz a cada execução da tarefa agendada.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn recorrencias_ativas(&self, empresa: Id) -> Resultado<Vec<Recorrencia>> {
        let mut stmt = self
            .conn()
            .prepare(
                "SELECT id, empresa, descricao, especie, contraparte_tipo, contraparte_id,
                        tipo_valor, valor_fixo, indice, media_ultimos_n, periodicidade,
                        dia_referencia, expressao_cron, inicio, fim, conta_contrapartida,
                        centro_custo, categoria, antecedencia_geracao_dias, ativa, versao
                 FROM financeiro_recorrencia WHERE empresa = ?1 AND ativa = 1",
            )
            .map_err(persist)?;
        let linhas = stmt
            .query_map([blob(empresa)], recorrencia_de_linha)
            .map_err(persist)?;
        linhas
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(persist)
    }

    /// Verdadeiro se já existe um título materializado desta recorrência para este
    /// vencimento — a guarda de idempotência de `materializar_recorrencias_pendentes`
    /// (a regra em si não guarda um cursor de "última ocorrência gerada").
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn recorrencia_ja_materializada_em(
        &self,
        recorrencia: Id,
        vencimento: Data,
    ) -> Resultado<bool> {
        self.conn()
            .query_row(
                "SELECT 1 FROM financeiro_titulo
                 WHERE origem_modulo = 'financeiro_recorrencia' AND origem_id = ?1 AND emissao = ?2
                 LIMIT 1",
                params![blob(recorrencia), dias(vencimento)],
                |_| Ok(()),
            )
            .optional()
            .map_err(persist)
            .map(|r| r.is_some())
    }

    /// Grava um caixa físico recém-cadastrado.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn inserir_caixa(&mut self, c: &Caixa) -> Resultado<()> {
        self.conn()
            .execute(
                "INSERT INTO financeiro_caixa
                   (id, empresa, nome, local_operacao, conta_razao, permite_negativo, ativo, versao)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
                params![
                    blob(c.id),
                    blob(c.empresa),
                    c.nome,
                    blob_opt(c.local_operacao),
                    blob(c.conta_razao),
                    i64::from(c.permite_negativo),
                    i64::from(c.ativo),
                    versao_i64(c.versao),
                ],
            )
            .map_err(persist)?;
        Ok(())
    }

    /// Busca um caixa pelo id. `Ok(None)` = não existe.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn buscar_caixa(&self, id: Id) -> Resultado<Option<Caixa>> {
        self.conn()
            .query_row(
                "SELECT id, empresa, nome, local_operacao, conta_razao, permite_negativo, ativo, versao
                 FROM financeiro_caixa WHERE id = ?1",
                [blob(id)],
                caixa_de_linha,
            )
            .optional()
            .map_err(persist)
    }

    /// A sessão em [`EstadoSessao::Aberta`] deste caixa, se houver — no máximo uma, garantido
    /// pelo índice único `financeiro_sessao_caixa_uma_aberta`.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn sessao_aberta_do_caixa(&self, caixa: Id) -> Resultado<Option<SessaoCaixa>> {
        self.conn()
            .query_row(
                "SELECT id, empresa, caixa, operador, dispositivo, abertura, fechamento,
                        valor_abertura, valor_esperado, valor_contado, quebra, estado, versao
                 FROM financeiro_sessao_caixa WHERE caixa = ?1 AND estado = 'Aberta'",
                [blob(caixa)],
                sessao_de_linha,
            )
            .optional()
            .map_err(persist)
    }

    /// Busca uma sessão de caixa pelo id. `Ok(None)` = não existe.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn buscar_sessao(&self, id: Id) -> Resultado<Option<SessaoCaixa>> {
        self.conn()
            .query_row(
                "SELECT id, empresa, caixa, operador, dispositivo, abertura, fechamento,
                        valor_abertura, valor_esperado, valor_contado, quebra, estado, versao
                 FROM financeiro_sessao_caixa WHERE id = ?1",
                [blob(id)],
                sessao_de_linha,
            )
            .optional()
            .map_err(persist)
    }

    /// Grava uma sessão de caixa recém-aberta.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn inserir_sessao(&mut self, s: &SessaoCaixa) -> Resultado<()> {
        self.conn()
            .execute(
                "INSERT INTO financeiro_sessao_caixa
                   (id, empresa, caixa, operador, dispositivo, abertura, fechamento,
                    valor_abertura, valor_esperado, valor_contado, quebra, estado, versao)
                 VALUES (?1,?2,?3,?4,?5,?6,NULL,?7,?8,?9,?10,?11,?12)",
                params![
                    blob(s.id),
                    blob(s.empresa),
                    blob(s.caixa),
                    blob(s.operador),
                    blob(s.dispositivo),
                    s.abertura.em_micros(),
                    s.valor_abertura.em_centavos(),
                    s.valor_esperado.map(Dinheiro::em_centavos),
                    s.valor_contado.map(Dinheiro::em_centavos),
                    s.quebra.map(Dinheiro::em_centavos),
                    estado_sessao_txt(s.estado),
                    versao_i64(s.versao),
                ],
            )
            .map_err(persist)?;
        Ok(())
    }

    /// Regrava uma sessão de caixa (fechamento, auditoria).
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn atualizar_sessao(&mut self, s: &SessaoCaixa) -> Resultado<()> {
        self.conn()
            .execute(
                "UPDATE financeiro_sessao_caixa
                 SET fechamento = ?2, valor_esperado = ?3, valor_contado = ?4, quebra = ?5,
                     estado = ?6, versao = ?7
                 WHERE id = ?1",
                params![
                    blob(s.id),
                    s.fechamento.map(Instante::em_micros),
                    s.valor_esperado.map(Dinheiro::em_centavos),
                    s.valor_contado.map(Dinheiro::em_centavos),
                    s.quebra.map(Dinheiro::em_centavos),
                    estado_sessao_txt(s.estado),
                    versao_i64(s.versao),
                ],
            )
            .map_err(persist)?;
        Ok(())
    }

    /// Grava um movimento de caixa. `m.lancamento` já deve estar preenchido — o comando
    /// registra o lançamento no Razão antes de persistir o movimento.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    ///
    /// # Panics
    /// Se `m.lancamento` for `None` — contrato do chamador, não uma condição do SQLite.
    pub fn inserir_movimento(&mut self, m: &MovimentoCaixa) -> Resultado<()> {
        self.conn()
            .execute(
                "INSERT INTO financeiro_movimento_caixa
                   (id, empresa, sessao, tipo, valor, forma_pagamento, lancamento, motivo,
                    criado_em, criado_por)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
                params![
                    blob(m.id),
                    blob(m.empresa),
                    blob(m.sessao),
                    tipo_movimento_txt(m.tipo),
                    m.valor.em_centavos(),
                    blob_opt(m.forma_pagamento),
                    blob(
                        m.lancamento
                            .expect("movimento só é gravado após o lançamento")
                    ),
                    m.motivo,
                    m.criado_em.em_micros(),
                    blob(m.criado_por),
                ],
            )
            .map_err(persist)?;
        Ok(())
    }

    /// Busca um movimento de caixa pelo id. `Ok(None)` = não existe.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn buscar_movimento(&self, id: Id) -> Resultado<Option<MovimentoCaixa>> {
        self.conn()
            .query_row(
                "SELECT id, empresa, sessao, tipo, valor, forma_pagamento, lancamento, motivo,
                        criado_em, criado_por
                 FROM financeiro_movimento_caixa WHERE id = ?1",
                [blob(id)],
                movimento_de_linha,
            )
            .optional()
            .map_err(persist)
    }
}

fn parcela_de_linha(r: &rusqlite::Row<'_>) -> rusqlite::Result<Parcela> {
    Ok(Parcela {
        id: id_de(r.get::<_, Vec<u8>>(0)?),
        empresa: id_de(r.get::<_, Vec<u8>>(1)?),
        titulo: id_de(r.get::<_, Vec<u8>>(2)?),
        numero: u16::try_from(r.get::<_, i64>(3)?).unwrap_or(0),
        vencimento: data_de(r.get::<_, i64>(4)?),
        valor: Dinheiro::centavos(r.get::<_, i64>(5)?),
        estado: estado_de(&r.get::<_, String>(6)?),
        valor_baixado: Dinheiro::centavos(r.get::<_, i64>(7)?),
        lancamento: r.get::<_, Option<Vec<u8>>>(8)?.map(id_de),
        nosso_numero: r.get::<_, Option<String>>(9)?,
        politica_juros: juros_de(&r.get::<_, String>(10)?),
        taxa_juros: r.get::<_, Option<i64>>(11)?.map(Percentual::unidades),
        multa: r.get::<_, Option<i64>>(12)?.map(Percentual::unidades),
        desconto_ate_data: r.get::<_, Option<i64>>(13)?.map(data_de),
        desconto_valor: r.get::<_, Option<i64>>(14)?.map(Dinheiro::centavos),
        versao: versao_de(r.get::<_, i64>(15)?),
    })
}

fn titulo_de_linha(r: &rusqlite::Row<'_>) -> rusqlite::Result<Titulo> {
    let cp_tipo: String = r.get(3)?;
    let cp_id: Vec<u8> = r.get(4)?;
    Ok(Titulo {
        id: id_de(r.get::<_, Vec<u8>>(0)?),
        empresa: id_de(r.get::<_, Vec<u8>>(1)?),
        especie: especie_de(&r.get::<_, String>(2)?),
        contraparte: contraparte_join(&cp_tipo, cp_id),
        origem_modulo: r.get(5)?,
        origem_id: r.get::<_, Option<Vec<u8>>>(6)?.map(id_de),
        emissao: data_de(r.get::<_, i64>(7)?),
        valor_original: Dinheiro::centavos(r.get::<_, i64>(8)?),
        forma_cobranca: forma_de(&r.get::<_, String>(9)?),
        centro_custo: r.get::<_, Option<Vec<u8>>>(10)?.map(id_de),
        categoria: r.get::<_, Option<Vec<u8>>>(11)?.map(id_de),
        observacao: r.get::<_, Option<String>>(12)?,
        cancelado_em: r
            .get::<_, Option<i64>>(13)?
            .map(cardeal_kernel::Instante::de_micros),
        versao: versao_de(r.get::<_, i64>(14)?),
    })
}

pub(crate) fn categoria_de_linha(r: &rusqlite::Row<'_>) -> rusqlite::Result<CategoriaFinanceira> {
    Ok(CategoriaFinanceira {
        id: id_de(r.get::<_, Vec<u8>>(0)?),
        empresa: id_de(r.get::<_, Vec<u8>>(1)?),
        nome: r.get(2)?,
        especie: especie_opt_de(r.get::<_, Option<String>>(3)?),
        ativa: r.get::<_, i64>(4)? != 0,
    })
}

pub(crate) fn recorrencia_de_linha(r: &rusqlite::Row<'_>) -> rusqlite::Result<Recorrencia> {
    let cp_tipo: String = r.get(4)?;
    let cp_id: Vec<u8> = r.get(5)?;
    Ok(Recorrencia {
        id: id_de(r.get::<_, Vec<u8>>(0)?),
        empresa: id_de(r.get::<_, Vec<u8>>(1)?),
        descricao: r.get(2)?,
        especie: especie_de(&r.get::<_, String>(3)?),
        contraparte: contraparte_join(&cp_tipo, cp_id),
        tipo_valor: tipo_valor_de(&r.get::<_, String>(6)?),
        valor_fixo: r.get::<_, Option<i64>>(7)?.map(Dinheiro::centavos),
        indice: r.get::<_, Option<String>>(8)?,
        media_ultimos_n: r
            .get::<_, Option<i64>>(9)?
            .map(|n| u16::try_from(n).unwrap_or(0)),
        periodicidade: periodicidade_de(&r.get::<_, String>(10)?),
        dia_referencia: r
            .get::<_, Option<i64>>(11)?
            .map(|d| u8::try_from(d).unwrap_or(0)),
        expressao_cron: r.get::<_, Option<String>>(12)?,
        inicio: data_de(r.get::<_, i64>(13)?),
        fim: r.get::<_, Option<i64>>(14)?.map(data_de),
        conta_contrapartida: id_de(r.get::<_, Vec<u8>>(15)?),
        centro_custo: r.get::<_, Option<Vec<u8>>>(16)?.map(id_de),
        categoria: r.get::<_, Option<Vec<u8>>>(17)?.map(id_de),
        antecedencia_geracao_dias: u16::try_from(r.get::<_, i64>(18)?).unwrap_or(0),
        ativa: r.get::<_, i64>(19)? != 0,
        versao: versao_de(r.get::<_, i64>(20)?),
    })
}

fn baixa_de_linha(r: &rusqlite::Row<'_>) -> rusqlite::Result<BaixaGravada> {
    Ok(BaixaGravada {
        id: id_de(r.get::<_, Vec<u8>>(0)?),
        empresa: id_de(r.get::<_, Vec<u8>>(1)?),
        parcela: id_de(r.get::<_, Vec<u8>>(2)?),
        data: data_de(r.get::<_, i64>(3)?),
        valor_recebido: Dinheiro::centavos(r.get::<_, i64>(4)?),
        principal: Dinheiro::centavos(r.get::<_, i64>(5)?),
        juros: Dinheiro::centavos(r.get::<_, i64>(6)?),
        multa: Dinheiro::centavos(r.get::<_, i64>(7)?),
        desconto: Dinheiro::centavos(r.get::<_, i64>(8)?),
        lancamento: id_de(r.get::<_, Vec<u8>>(9)?),
        estornada_em: r.get::<_, Option<i64>>(10)?.map(Instante::de_micros),
    })
}

fn caixa_de_linha(r: &rusqlite::Row<'_>) -> rusqlite::Result<Caixa> {
    Ok(Caixa {
        id: id_de(r.get::<_, Vec<u8>>(0)?),
        empresa: id_de(r.get::<_, Vec<u8>>(1)?),
        nome: r.get(2)?,
        local_operacao: r.get::<_, Option<Vec<u8>>>(3)?.map(id_de),
        conta_razao: id_de(r.get::<_, Vec<u8>>(4)?),
        permite_negativo: r.get::<_, i64>(5)? != 0,
        ativo: r.get::<_, i64>(6)? != 0,
        versao: versao_de(r.get::<_, i64>(7)?),
    })
}

fn sessao_de_linha(r: &rusqlite::Row<'_>) -> rusqlite::Result<SessaoCaixa> {
    Ok(SessaoCaixa {
        id: id_de(r.get::<_, Vec<u8>>(0)?),
        empresa: id_de(r.get::<_, Vec<u8>>(1)?),
        caixa: id_de(r.get::<_, Vec<u8>>(2)?),
        operador: id_de(r.get::<_, Vec<u8>>(3)?),
        dispositivo: id_de(r.get::<_, Vec<u8>>(4)?),
        abertura: Instante::de_micros(r.get::<_, i64>(5)?),
        fechamento: r.get::<_, Option<i64>>(6)?.map(Instante::de_micros),
        valor_abertura: Dinheiro::centavos(r.get::<_, i64>(7)?),
        valor_esperado: r.get::<_, Option<i64>>(8)?.map(Dinheiro::centavos),
        valor_contado: r.get::<_, Option<i64>>(9)?.map(Dinheiro::centavos),
        quebra: r.get::<_, Option<i64>>(10)?.map(Dinheiro::centavos),
        estado: estado_sessao_de(&r.get::<_, String>(11)?),
        versao: versao_de(r.get::<_, i64>(12)?),
    })
}

fn movimento_de_linha(r: &rusqlite::Row<'_>) -> rusqlite::Result<MovimentoCaixa> {
    Ok(MovimentoCaixa {
        id: id_de(r.get::<_, Vec<u8>>(0)?),
        empresa: id_de(r.get::<_, Vec<u8>>(1)?),
        sessao: id_de(r.get::<_, Vec<u8>>(2)?),
        tipo: tipo_movimento_de(&r.get::<_, String>(3)?),
        valor: Dinheiro::centavos(r.get::<_, i64>(4)?),
        forma_pagamento: r.get::<_, Option<Vec<u8>>>(5)?.map(id_de),
        lancamento: Some(id_de(r.get::<_, Vec<u8>>(6)?)),
        motivo: r.get::<_, Option<String>>(7)?,
        criado_em: Instante::de_micros(r.get::<_, i64>(8)?),
        criado_por: id_de(r.get::<_, Vec<u8>>(9)?),
    })
}
