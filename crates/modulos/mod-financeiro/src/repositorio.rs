//! Todo o SQL do módulo financeiro (`docs/15-convencoes-codigo.md` §5): um único lugar com
//! strings SQL, sempre colunas explícitas, sempre parâmetros nomeados ou posicionais fixos.
//!
//! [`RepositorioFinanceiro`] grava e lê `financeiro_titulo`/`financeiro_parcela`/
//! `financeiro_baixa` sobre a [`UnidadeDeTrabalho`](cardeal_storage::UnidadeDeTrabalho) do
//! escritor único — mesmo padrão do `RepositorioRazao`.

use cardeal_kernel::{CodigoErro, Data, Dinheiro, Erro, Id, Percentual, Resultado, Versao};
use cardeal_ledger::Contraparte;
use cardeal_storage::UnidadeDeTrabalho;
use rusqlite::{params, Connection, OptionalExtension};

use crate::titulo::{
    EspecieTitulo, EstadoParcela, FormaCobranca, Parcela, PoliticaJuros, Titulo, TituloComParcelas,
};

#[allow(clippy::needless_pass_by_value)] // usado como `.map_err(persist)`
fn persist(e: rusqlite::Error) -> Erro {
    Erro::novo(CodigoErro::FALHA_INTERNA, format!("financeiro/SQL: {e}"))
}

fn blob(id: Id) -> Vec<u8> {
    id.em_bytes().to_vec()
}

fn blob_opt(id: Option<Id>) -> Option<Vec<u8>> {
    id.map(blob)
}

fn id_de(bytes: Vec<u8>) -> Id {
    <[u8; 16]>::try_from(bytes).map_or(Id::NULO, Id::de_bytes)
}

fn dias(d: Data) -> i64 {
    i64::from(d.em_dias())
}

fn data_de(dias: i64) -> Data {
    Data::de_dias(i32::try_from(dias).unwrap_or(0))
}

fn versao_de(n: i64) -> Versao {
    Versao::nova(u64::try_from(n).unwrap_or(1))
}

fn versao_i64(v: Versao) -> i64 {
    i64::try_from(v.numero()).unwrap_or(i64::MAX)
}

fn especie_txt(e: EspecieTitulo) -> &'static str {
    match e {
        EspecieTitulo::Receber => "Receber",
        EspecieTitulo::Pagar => "Pagar",
    }
}

fn especie_de(s: &str) -> EspecieTitulo {
    match s {
        "Pagar" => EspecieTitulo::Pagar,
        _ => EspecieTitulo::Receber,
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

fn estado_de(s: &str) -> EstadoParcela {
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

fn contraparte_join(tipo: &str, id: Vec<u8>) -> Contraparte {
    let id = id_de(id);
    match tipo {
        "Fornecedor" => Contraparte::Fornecedor(id),
        "Funcionario" => Contraparte::Funcionario(id),
        "Socio" => Contraparte::Socio(id),
        "Outro" => Contraparte::Outro(id),
        _ => Contraparte::Cliente(id),
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
                    origem_id, emissao, valor_original, forma_cobranca, centro_custo,
                    observacao, cancelado_em, versao, criado_em, criado_por)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,NULL,?13,?14,?15)",
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
                        observacao, cancelado_em, versao
                 FROM financeiro_titulo WHERE id = ?1",
                [blob(id)],
                titulo_de_linha,
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
        observacao: r.get::<_, Option<String>>(11)?,
        cancelado_em: r
            .get::<_, Option<i64>>(12)?
            .map(cardeal_kernel::Instante::de_micros),
        versao: versao_de(r.get::<_, i64>(13)?),
    })
}
