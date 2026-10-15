//! A NFC-e das vendas do PDV: configuração (CSC no cofre, série, próximo número), o documento
//! criado na hora da venda (rápido, sem rede — **a venda nunca espera a SEFAZ**, `docs/10` §3)
//! e a **tarefa** que transmite a fila fora de qualquer transação.

use cardeal_fiscal::{AmbienteFiscal, Nfce};
use cardeal_kernel::Id;
#[cfg(feature = "sqlite")]
use cardeal_kernel::Resultado;
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
use cardeal_modkit::{Comando, Consulta, Risco};
#[cfg(feature = "sqlite")]
use cardeal_storage::UnidadeDeTrabalho;
#[cfg(feature = "sqlite")]
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::documento::{DocumentoResumo, EstadoDocumento, ResumoFiscal};

/// A configuração da NFC-e, para a tela.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfiguracaoNfce {
    /// Emitir NFC-e automaticamente a cada venda do PDV.
    pub emitir_nas_vendas: bool,
    /// A série.
    pub serie: u32,
    /// O próximo número da série, no ambiente atual.
    pub proximo_numero: u64,
    /// O identificador do CSC.
    pub csc_id: String,
    /// Se o token do CSC está guardado.
    pub csc_cadastrado: bool,
    /// O ambiente atual (a numeração é separada por ambiente).
    pub ambiente: AmbienteFiscal,
}

/// Configura a NFC-e. O token do CSC vai para o cofre; `None` mantém o guardado.
#[derive(Clone, Serialize, Deserialize)]
pub struct ConfigurarNfce {
    /// Emitir a cada venda do PDV.
    pub emitir_nas_vendas: bool,
    /// A série (1 a 889).
    pub serie: u32,
    /// O próximo número no ambiente atual (≥ 1, e acima do maior já usado).
    pub proximo_numero: u64,
    /// O identificador do CSC (ex.: "000001").
    pub csc_id: String,
    /// O token do CSC; `None` mantém o guardado.
    pub csc_token: Option<String>,
}

impl std::fmt::Debug for ConfigurarNfce {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConfigurarNfce")
            .field("emitir_nas_vendas", &self.emitir_nas_vendas)
            .field("serie", &self.serie)
            .field("proximo_numero", &self.proximo_numero)
            .field("csc_id", &self.csc_id)
            .field("csc_token", &self.csc_token.as_ref().map(|_| "[oculto]"))
            .finish()
    }
}

impl Drop for ConfigurarNfce {
    fn drop(&mut self) {
        if let Some(t) = self.csc_token.as_mut() {
            zeroize::Zeroize::zeroize(t);
        }
    }
}

impl Comando for ConfigurarNfce {
    type Saida = ConfiguracaoNfce;
    const PERMISSAO: &'static str = "fiscal.configurar";
    const RISCO: Risco = Risco::Alto;
    const AUDITA: bool = true;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<ConfiguracaoNfce> {
        sqlite::configurar(&self, ctx, uow)
    }
}

/// A configuração atual da NFC-e.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct ConfiguracaoDaNfce;

impl Consulta for ConfiguracaoDaNfce {
    type Saida = ConfiguracaoNfce;
    const PERMISSAO: &'static str = "fiscal.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, c: &Connection) -> Resultado<ConfiguracaoNfce> {
        sqlite::configuracao(ctx, c)
    }
}

/// Cria a NFC-e de uma venda finalizada do PDV: reserva o número e guarda a nota montada — ou,
/// se faltar algo (produto sem NCM, configuração incompleta), o documento já nasce rejeitado
/// com o que corrigir. Não fala com a rede. Repetir devolve o mesmo documento.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct EmitirNfceDoCupom {
    /// O cupom (finalizado).
    pub cupom: Id,
}

impl Comando for EmitirNfceDoCupom {
    type Saida = DocumentoResumo;
    const PERMISSAO: &'static str = "fiscal.emitir";
    const RISCO: Risco = Risco::Medio;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<DocumentoResumo> {
        sqlite::emitir_do_cupom(ctx, uow, self.cupom)
    }
}

/// Remonta um documento rejeitado (depois de corrigir o produto ou a configuração) e o põe de
/// volta na fila, com o mesmo número.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ReenviarDocumento {
    /// O documento.
    pub documento: Id,
}

impl Comando for ReenviarDocumento {
    type Saida = DocumentoResumo;
    const PERMISSAO: &'static str = "fiscal.emitir";
    const RISCO: Risco = Risco::Medio;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<DocumentoResumo> {
        sqlite::reenviar(ctx, uow, self.documento)
    }
}

/// Pede o cancelamento de uma NFC-e autorizada. A fila leva à SEFAZ (que aceita dentro do
/// prazo da UF — em geral 30 minutos para a NFC-e); recusado, a nota continua autorizada com o
/// motivo. A venda em si não é desfeita aqui.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelarNfce {
    /// O documento (autorizado).
    pub documento: Id,
    /// A justificativa (15 a 255 caracteres).
    pub justificativa: String,
}

impl Comando for CancelarNfce {
    type Saida = DocumentoResumo;
    const PERMISSAO: &'static str = "fiscal.cancelar";
    const RISCO: Risco = Risco::Alto;
    const AUDITA: bool = true;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<DocumentoResumo> {
        sqlite::pedir(
            ctx,
            uow,
            self.documento,
            crate::documento::PedidoDocumento::Cancelar,
            &self.justificativa,
        )
    }
}

/// Pede a inutilização do número de uma NFC-e rejeitada que não vai mais ser enviada — sem
/// isso, o número fica como um buraco na série.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InutilizarNumero {
    /// O documento (rejeitado).
    pub documento: Id,
    /// A justificativa (15 a 255 caracteres).
    pub justificativa: String,
}

impl Comando for InutilizarNumero {
    type Saida = DocumentoResumo;
    const PERMISSAO: &'static str = "fiscal.cancelar";
    const RISCO: Risco = Risco::Alto;
    const AUDITA: bool = true;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<DocumentoResumo> {
        sqlite::pedir(
            ctx,
            uow,
            self.documento,
            crate::documento::PedidoDocumento::Inutilizar,
            &self.justificativa,
        )
    }
}

/// A venda do PDV foi estornada: a NFC-e dela tem de sair de cena. Marca o documento (de forma
/// permanente) e pede o que o estado dele pede — cancelar a autorizada, inutilizar a rejeitada
/// e a que nunca foi transmitida; a incerta e a pendente já tentada a fila resolve primeiro
/// (emite ou confere) e só então cancela ou inutiliza. Só vale para venda **estornada**: é a
/// consequência do estorno, por isso basta a permissão de emitir.
///
/// Devolve o documento, ou `None` se a venda não tinha NFC-e.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnularDocumentoDaVenda {
    /// O cupom estornado.
    pub cupom: Id,
    /// A justificativa que vai ao cancelamento ou à inutilização (15 a 255 caracteres).
    pub justificativa: String,
}

impl Comando for AnularDocumentoDaVenda {
    type Saida = Option<DocumentoResumo>;
    const PERMISSAO: &'static str = "fiscal.emitir";
    const RISCO: Risco = Risco::Medio;
    const AUDITA: bool = true;

    #[cfg(feature = "sqlite")]
    fn executar(
        self,
        ctx: &Ctx,
        uow: &mut UnidadeDeTrabalho,
    ) -> Resultado<Option<DocumentoResumo>> {
        sqlite::anular(ctx, uow, self.cupom, &self.justificativa)
    }
}

/// As NFC-e de várias vendas de uma vez (a lista "Vendas do dia" do PDV).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentosDasVendas {
    /// Os cupons.
    pub cupons: Vec<Id>,
}

impl Consulta for DocumentosDasVendas {
    type Saida = Vec<DocumentoResumo>;
    const PERMISSAO: &'static str = "fiscal.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, c: &Connection) -> Resultado<Vec<DocumentoResumo>> {
        crate::sql_documento::documentos_das_origens(
            c,
            ctx.empresa,
            sqlite::ORIGEM_PDV,
            &self.cupons,
        )
    }
}

/// Os documentos fiscais, os mais novos primeiro (até 500).
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct DocumentosFiscais {
    /// Só os deste estado.
    pub estado: Option<EstadoDocumento>,
}

impl Consulta for DocumentosFiscais {
    type Saida = Vec<DocumentoResumo>;
    const PERMISSAO: &'static str = "fiscal.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, c: &Connection) -> Resultado<Vec<DocumentoResumo>> {
        crate::sql_documento::documentos(c, ctx.empresa, self.estado, 500)
    }
}

/// O resumo da fila (a faixa de estado do PDV).
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct ResumoDaFilaFiscal;

impl Consulta for ResumoDaFilaFiscal {
    type Saida = ResumoFiscal;
    const PERMISSAO: &'static str = "fiscal.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, c: &Connection) -> Resultado<ResumoFiscal> {
        let inicio = cardeal_kernel::Instante::de_data_hora(
            ctx.hoje(),
            cardeal_kernel::Hora::MEIA_NOITE,
            ctx.fuso,
        );
        crate::sql_documento::resumo(c, ctx.empresa, inicio)
    }
}

/// O documento com a nota e o XML.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DetalheDocumento {
    /// O resumo.
    pub resumo: DocumentoResumo,
    /// A nota como foi montada (`None` se a montagem falhou).
    pub nota: Option<Nfce>,
    /// O XML autorizado.
    pub xml: Option<String>,
}

/// Um documento fiscal.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DocumentoFiscal {
    /// O documento.
    pub documento: Id,
}

impl Consulta for DocumentoFiscal {
    type Saida = DetalheDocumento;
    const PERMISSAO: &'static str = "fiscal.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, c: &Connection) -> Resultado<DetalheDocumento> {
        let d = crate::sql_documento::documento(c, ctx.empresa, self.documento)?
            .ok_or_else(|| cardeal_kernel::Erro::nao_encontrado("documento fiscal"))?;
        Ok(DetalheDocumento {
            resumo: d.resumo,
            nota: d.nota.and_then(|b| postcard::from_bytes(&b).ok()),
            xml: d.xml,
        })
    }
}

/// A NFC-e de um cupom, se houver.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DocumentoDaVenda {
    /// O cupom.
    pub cupom: Id,
}

impl Consulta for DocumentoDaVenda {
    type Saida = Option<DocumentoResumo>;
    const PERMISSAO: &'static str = "fiscal.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, c: &Connection) -> Resultado<Option<DocumentoResumo>> {
        crate::sql_documento::documento_da_origem(c, ctx.empresa, sqlite::ORIGEM_PDV, self.cupom)
    }
}

/// O nome da tarefa que transmite a fila.
pub const TAREFA_TRANSMITIR: &str = "fiscal.transmitir_pendentes";

#[cfg(feature = "sqlite")]
pub(crate) mod sqlite {
    use std::collections::{HashMap, HashSet};
    use std::fmt::Write as _;
    use std::sync::Mutex;

    use cardeal_fiscal::{
        AmbienteFiscal, ChaveAcesso, Consumidor, Crt, Csc, DocumentoConsumidor, Emitente,
        EnderecoEmitente, MeioPagamento, ModeloFiscal, Nfce, PortaFiscal,
    };
    use cardeal_kernel::{CodigoErro, Erro, Id, Instante, Resultado, Uf};
    use cardeal_modkit::{ContextoTarefa, Ctx};
    use cardeal_storage::{Cofre, Segredo, UnidadeDeTrabalho};
    use rusqlite::Connection;

    use super::{ConfiguracaoNfce, ConfigurarNfce};
    use crate::documento::{
        decidir, decidir_evento, Desfecho, DocumentoResumo, EstadoDocumento, PedidoDocumento,
    };
    use crate::montagem::{marcar_homologacao, montar_nfce, DadosDaVenda, ItemDaVenda};
    use crate::sql_documento as sqld;

    pub(crate) const ORIGEM_PDV: &str = "pdv";
    const MODELO: ModeloFiscal = ModeloFiscal::Nfce;

    fn contexto_csc(empresa: Id) -> Vec<u8> {
        let mut c = b"fiscal.csc/".to_vec();
        c.extend_from_slice(&empresa.em_bytes());
        c
    }

    pub(super) fn configurar(
        cmd: &ConfigurarNfce,
        ctx: &Ctx,
        uow: &mut UnidadeDeTrabalho,
    ) -> Resultado<ConfiguracaoNfce> {
        let c = uow.conexao();
        if !(1..=889).contains(&cmd.serie) {
            return Err(
                Erro::novo(CodigoErro::FORA_DA_FAIXA, "A série da NFC-e vai de 1 a 889")
                    .no_campo("serie"),
            );
        }
        if cmd.proximo_numero == 0 || cmd.proximo_numero > 999_999_999 {
            return Err(Erro::novo(
                CodigoErro::FORA_DA_FAIXA,
                "O próximo número vai de 1 a 999.999.999",
            )
            .no_campo("proximo_numero"));
        }
        let csc_id = cmd.csc_id.trim();
        if !csc_id.is_empty() && (csc_id.len() > 6 || !csc_id.bytes().all(|b| b.is_ascii_digit())) {
            return Err(Erro::novo(
                CodigoErro::VALOR_INVALIDO,
                "O identificador do CSC tem até 6 dígitos (ex.: 000001)",
            )
            .no_campo("csc_id"));
        }
        let atual = sqld::config_nfce(c, ctx.empresa)?;
        let token = cmd
            .csc_token
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty());
        let tera_token = token.is_some() || atual.csc_selado.is_some();
        if cmd.emitir_nas_vendas && (csc_id.is_empty() || !tera_token) {
            return Err(Erro::novo(
                CodigoErro::CAMPO_OBRIGATORIO,
                "Para emitir NFC-e nas vendas, informe o CSC (identificador e token) da SEFAZ",
            )
            .no_campo("csc_id"));
        }
        let ambiente = crate::sql::ambiente(c, ctx.empresa)?;
        if let Some(maior) = sqld::maior_numero(c, ctx.empresa, MODELO, cmd.serie, ambiente)? {
            if cmd.proximo_numero <= maior {
                return Err(Erro::novo(
                    CodigoErro::REGRA_VIOLADA,
                    format!(
                        "O número {maior} já foi usado na série {} — o próximo precisa ser maior",
                        cmd.serie
                    ),
                )
                .no_campo("proximo_numero"));
            }
        }
        let selado = match token {
            Some(t) => Some(
                ctx.recurso::<Cofre>()?
                    .selar(t.as_bytes(), &contexto_csc(ctx.empresa)),
            ),
            None => None,
        };
        sqld::gravar_config_nfce(
            c,
            ctx.empresa,
            cmd.emitir_nas_vendas,
            cmd.serie,
            csc_id,
            selado.as_deref(),
        )?;
        sqld::definir_proximo(
            c,
            ctx.empresa,
            MODELO,
            cmd.serie,
            ambiente,
            cmd.proximo_numero,
        )?;
        configuracao(ctx, c)
    }

    pub(super) fn configuracao(ctx: &Ctx, c: &Connection) -> Resultado<ConfiguracaoNfce> {
        let g = sqld::config_nfce(c, ctx.empresa)?;
        let ambiente = crate::sql::ambiente(c, ctx.empresa)?;
        Ok(ConfiguracaoNfce {
            emitir_nas_vendas: g.emitir,
            serie: g.serie,
            proximo_numero: sqld::proximo_numero(c, ctx.empresa, MODELO, g.serie, ambiente)?,
            csc_id: g.csc_id.unwrap_or_default(),
            csc_cadastrado: g.csc_selado.is_some(),
            ambiente,
        })
    }

    /// O emitente a partir dos dados da empresa — ou o que falta.
    pub(crate) fn emitente(
        c: &Connection,
        empresa: Id,
    ) -> Resultado<Result<Emitente, Vec<String>>> {
        let dados = mod_empresa::sql::dados(c, empresa)?;
        let Ok(fiscais) = mod_empresa::sql::dados_fiscais(c, empresa)?.normalizado() else {
            return Ok(Err(vec![
                "os dados fiscais da empresa estão incompletos (Configurações › Fiscal)".into(),
            ]));
        };
        let Some(uf) = fiscais.uf() else {
            return Ok(Err(vec!["a UF da empresa é inválida".into()]));
        };
        let Ok(cnpj) = cardeal_kernel::Cnpj::novo(&dados.cnpj) else {
            return Ok(Err(vec!["o CNPJ da empresa é inválido".into()]));
        };
        let crt = match dados.regime.as_str() {
            "LucroPresumido" | "LucroReal" => Crt::RegimeNormal,
            _ => Crt::SimplesNacional,
        };
        let telefone: String = mod_empresa::sql::identidade(c, empresa)?
            .telefone
            .chars()
            .filter(char::is_ascii_digit)
            .collect();
        let e = fiscais.endereco;
        Ok(Ok(Emitente {
            cnpj,
            razao_social: dados.razao_social.trim().chars().take(60).collect(),
            nome_fantasia: dados.nome_fantasia.trim().chars().take(60).collect(),
            inscricao_estadual: fiscais.inscricao_estadual,
            inscricao_municipal: Some(fiscais.inscricao_municipal).filter(|s| !s.is_empty()),
            crt,
            endereco: EnderecoEmitente {
                logradouro: e.logradouro,
                numero: e.numero,
                complemento: e.complemento,
                bairro: e.bairro,
                codigo_municipio: e.codigo_municipio,
                municipio: e.municipio,
                uf,
                cep: e.cep,
                telefone: Some(telefone).filter(|t| (10..=14).contains(&t.len())),
            },
        }))
    }

    /// Monta a nota de um cupom com a série/número dados — ou o que impede.
    fn montar_do_cupom(
        ctx: &Ctx,
        uow: &mut UnidadeDeTrabalho,
        cupom: Id,
        serie: u32,
        numero: u64,
        ambiente: AmbienteFiscal,
    ) -> Resultado<(cardeal_kernel::Dinheiro, Result<Nfce, Vec<String>>)> {
        let cupom = mod_pdv::RepositorioPdv::novo(uow)
            .buscar_cupom(cupom)?
            .ok_or_else(|| Erro::nao_encontrado("cupom"))?;
        if cupom.estado != mod_pdv::EstadoCupom::Finalizado {
            return Err(Erro::novo(
                CodigoErro::ESTADO_INVALIDO,
                "Só uma venda finalizada vira NFC-e",
            ));
        }
        let c = uow.conexao();
        let itens: Vec<ItemDaVenda> = cupom
            .itens
            .iter()
            .filter(|i| !i.cancelado)
            .map(|i| ItemDaVenda {
                produto: i.produto,
                quantidade: i.quantidade,
                preco_unitario: i.preco_unitario,
                valor_bruto: i.total_bruto(),
                desconto: i.desconto_valor,
            })
            .collect();
        let pagamentos = cupom
            .pagamentos
            .iter()
            .map(|p| {
                let meio = match p.forma {
                    mod_pdv::FormaPagamentoPdv::Dinheiro => MeioPagamento::Dinheiro,
                    mod_pdv::FormaPagamentoPdv::Pix => MeioPagamento::Pix,
                    mod_pdv::FormaPagamentoPdv::Debito => MeioPagamento::CartaoDebito,
                    mod_pdv::FormaPagamentoPdv::Credito => MeioPagamento::CartaoCredito,
                };
                (meio, p.valor)
            })
            .collect();
        let consumidor = match cupom.cliente {
            Some(cliente) => consumidor(c, cliente)?,
            None => None,
        };
        let ids: Vec<Id> = itens.iter().map(|i| i.produto).collect();
        let produtos = mod_estoque::produtos_para_nota(c, ctx.organizacao, &ids)?;
        let mut problemas = pendencias_de_configuracao(ctx, c)?;
        let nota = match emitente(c, ctx.empresa)? {
            Ok(emitente) => montar_nfce(
                emitente,
                &DadosDaVenda {
                    itens,
                    pagamentos,
                    consumidor,
                },
                &produtos,
                serie,
                numero,
            ),
            Err(p) => Err(p),
        };
        let nota = match nota {
            Ok(mut n) if problemas.is_empty() => {
                if ambiente == AmbienteFiscal::Homologacao {
                    marcar_homologacao(&mut n);
                }
                Ok(n)
            }
            Ok(_) => Err(problemas),
            Err(mut p) => {
                problemas.append(&mut p);
                Err(problemas)
            }
        };
        Ok((cupom.total, nota))
    }

    fn consumidor(c: &Connection, cliente: Id) -> Resultado<Option<Consumidor>> {
        let documento = mod_clientes::documentos_da_pessoa(c, cliente)?
            .into_iter()
            .find_map(|d| match d.tipo {
                mod_clientes::TipoDocumento::Cpf => cardeal_kernel::Cpf::novo(&d.numero)
                    .ok()
                    .map(DocumentoConsumidor::Cpf),
                mod_clientes::TipoDocumento::Cnpj => cardeal_kernel::Cnpj::novo(&d.numero)
                    .ok()
                    .map(DocumentoConsumidor::Cnpj),
                _ => None,
            });
        let Some(documento) = documento else {
            return Ok(None);
        };
        let nome = mod_clientes::pessoa_por_id(c, cliente)?
            .map(|p| p.nome.trim().chars().take(60).collect::<String>())
            .filter(|n| n.len() >= 2);
        Ok(Some(Consumidor { documento, nome }))
    }

    /// O que falta na instalação para transmitir (certificado, CSC).
    fn pendencias_de_configuracao(ctx: &Ctx, c: &Connection) -> Resultado<Vec<String>> {
        let mut p = Vec::new();
        if crate::sql::certificado(c, ctx.empresa)?.is_none() {
            p.push("o certificado digital A1 não está instalado (Configurações › Fiscal)".into());
        }
        let config = sqld::config_nfce(c, ctx.empresa)?;
        if config.csc_selado.is_none() || config.csc_id.as_deref().is_none_or(str::is_empty) {
            p.push("o CSC da NFC-e não está cadastrado (Configurações › Fiscal)".into());
        }
        Ok(p)
    }

    pub(super) fn emitir_do_cupom(
        ctx: &Ctx,
        uow: &mut UnidadeDeTrabalho,
        cupom: Id,
    ) -> Resultado<DocumentoResumo> {
        if let Some(d) = sqld::documento_da_origem(uow.conexao(), ctx.empresa, ORIGEM_PDV, cupom)? {
            return Ok(d);
        }
        let (serie, ambiente) = {
            let c = uow.conexao();
            (
                sqld::config_nfce(c, ctx.empresa)?.serie,
                crate::sql::ambiente(c, ctx.empresa)?,
            )
        };
        let numero = sqld::reservar_numero(uow.conexao(), ctx.empresa, MODELO, serie, ambiente)?;
        let (total, nota) = montar_do_cupom(ctx, uow, cupom, serie, numero, ambiente)?;
        let (estado, blob, motivo) = match nota {
            Ok(n) => (
                EstadoDocumento::Pendente,
                Some(
                    postcard::to_stdvec(&n)
                        .map_err(|e| Erro::novo(CodigoErro::FALHA_INTERNA, e.to_string()))?,
                ),
                None,
            ),
            Err(p) => (EstadoDocumento::Rejeitada, None, Some(p.join("; "))),
        };
        let id = Id::novo();
        let c = uow.conexao();
        sqld::inserir_documento(
            c,
            &sqld::NovoDocumento {
                id,
                empresa: ctx.empresa,
                modelo: MODELO,
                serie,
                numero,
                ambiente,
                origem_modulo: ORIGEM_PDV,
                origem: cupom,
                estado,
                total,
                nota: blob,
                motivo,
                agora: ctx.agora,
            },
        )?;
        sqld::documento_da_origem(c, ctx.empresa, ORIGEM_PDV, cupom)?
            .ok_or_else(|| Erro::nao_encontrado("documento fiscal"))
    }

    pub(super) fn reenviar(
        ctx: &Ctx,
        uow: &mut UnidadeDeTrabalho,
        documento: Id,
    ) -> Resultado<DocumentoResumo> {
        let guardado = sqld::documento(uow.conexao(), ctx.empresa, documento)?
            .ok_or_else(|| Erro::nao_encontrado("documento fiscal"))?;
        let (resumo, origem_modulo) = (guardado.resumo, guardado.origem_modulo);
        if resumo.estado != EstadoDocumento::Rejeitada {
            return Err(Erro::novo(
                CodigoErro::ESTADO_INVALIDO,
                format!(
                    "Só nota rejeitada é reenviada (esta está {})",
                    resumo.estado.rotulo()
                ),
            ));
        }
        if resumo.anulada || resumo.pedido.is_some() {
            return Err(Erro::novo(
                CodigoErro::ESTADO_INVALIDO,
                "Esta nota não se reenvia: a venda foi desfeita ou o número já vai ser inutilizado",
            ));
        }
        if origem_modulo != ORIGEM_PDV {
            return Err(Erro::novo(
                CodigoErro::ESTADO_INVALIDO,
                "Origem do documento desconhecida",
            ));
        }
        let (total, nota) = montar_do_cupom(
            ctx,
            uow,
            resumo.origem,
            resumo.serie,
            resumo.numero,
            resumo.ambiente,
        )?;
        let (blob, motivo) = match nota {
            Ok(n) => (
                Some(
                    postcard::to_stdvec(&n)
                        .map_err(|e| Erro::novo(CodigoErro::FALHA_INTERNA, e.to_string()))?,
                ),
                None,
            ),
            Err(p) => (None, Some(p.join("; "))),
        };
        let c = uow.conexao();
        sqld::remontar(
            c,
            ctx.empresa,
            documento,
            blob.as_deref(),
            total,
            motivo.as_deref(),
            ctx.agora,
        )?;
        sqld::documento(c, ctx.empresa, documento)?
            .map(|d| d.resumo)
            .ok_or_else(|| Erro::nao_encontrado("documento fiscal"))
    }

    /// A justificativa da SEFAZ: de 15 a 255 caracteres.
    fn justificativa_valida(justificativa: &str) -> Resultado<&str> {
        let justificativa = justificativa.trim();
        let n = justificativa.chars().count();
        if (15..=255).contains(&n) {
            Ok(justificativa)
        } else {
            Err(Erro::novo(
                CodigoErro::VALOR_INVALIDO,
                "A justificativa precisa ter de 15 a 255 caracteres",
            )
            .no_campo("justificativa"))
        }
    }

    pub(super) fn anular(
        ctx: &Ctx,
        uow: &mut UnidadeDeTrabalho,
        cupom: Id,
        justificativa: &str,
    ) -> Resultado<Option<DocumentoResumo>> {
        let justificativa = justificativa_valida(justificativa)?;
        let venda = mod_pdv::RepositorioPdv::novo(uow)
            .buscar_cupom(cupom)?
            .ok_or_else(|| Erro::nao_encontrado("venda"))?;
        if venda.estado != mod_pdv::EstadoCupom::Estornado {
            return Err(Erro::novo(
                CodigoErro::ESTADO_INVALIDO,
                "A venda não foi estornada: a NFC-e só se anula junto com o estorno da venda",
            ));
        }
        let c = uow.conexao();
        let Some(doc) = sqld::documento_da_origem(c, ctx.empresa, ORIGEM_PDV, cupom)? else {
            return Ok(None);
        };
        if !doc.anulada {
            let pedido = crate::documento::pedido_da_anulacao(doc.estado, doc.tentativas)
                .filter(|_| doc.pedido.is_none());
            sqld::anular(c, ctx.empresa, doc.id, justificativa, pedido, ctx.agora)?;
        }
        sqld::documento_da_origem(c, ctx.empresa, ORIGEM_PDV, cupom)
    }

    pub(super) fn pedir(
        ctx: &Ctx,
        uow: &mut UnidadeDeTrabalho,
        documento: Id,
        pedido: PedidoDocumento,
        justificativa: &str,
    ) -> Resultado<DocumentoResumo> {
        let justificativa = justificativa_valida(justificativa)?;
        let c = uow.conexao();
        let guardado = sqld::documento(c, ctx.empresa, documento)?
            .ok_or_else(|| Erro::nao_encontrado("documento fiscal"))?;
        let resumo = guardado.resumo;
        let exigido = match pedido {
            PedidoDocumento::Cancelar => EstadoDocumento::Autorizada,
            PedidoDocumento::Inutilizar => EstadoDocumento::Rejeitada,
        };
        if resumo.estado != exigido {
            return Err(Erro::novo(
                CodigoErro::ESTADO_INVALIDO,
                match pedido {
                    PedidoDocumento::Cancelar => "Só NFC-e autorizada é cancelada",
                    PedidoDocumento::Inutilizar => {
                        "Só o número de uma NFC-e rejeitada (que nunca foi autorizada) é inutilizado"
                    }
                },
            ));
        }
        if let Some(p) = resumo.pedido {
            return Err(Erro::novo(
                CodigoErro::ESTADO_INVALIDO,
                format!("Já há um pedido para esta nota: {}", p.rotulo()),
            ));
        }
        sqld::pedir(
            c,
            ctx.empresa,
            documento,
            pedido,
            justificativa,
            exigido,
            ctx.agora,
        )?;
        sqld::documento(c, ctx.empresa, documento)?
            .map(|d| d.resumo)
            .ok_or_else(|| Erro::nao_encontrado("documento fiscal"))
    }

    /// As empresas com transmissão em andamento neste processo — duas tarefas ao mesmo tempo
    /// emitiriam a mesma nota duas vezes.
    static EM_ANDAMENTO: Mutex<Option<HashSet<Id>>> = Mutex::new(None);

    struct Guarda(Id);

    impl Guarda {
        fn tomar(empresa: Id) -> Option<Self> {
            let mut g = EM_ANDAMENTO
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            g.get_or_insert_with(HashSet::new)
                .insert(empresa)
                .then_some(Self(empresa))
        }
    }

    impl Drop for Guarda {
        fn drop(&mut self) {
            let mut g = EM_ANDAMENTO
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(s) = g.as_mut() {
                s.remove(&self.0);
            }
        }
    }

    fn csc(ctx: &Ctx, c: &Connection) -> Resultado<Option<Csc>> {
        let config = sqld::config_nfce(c, ctx.empresa)?;
        let (Some(id), Some(selado)) = (config.csc_id.filter(|i| !i.is_empty()), config.csc_selado)
        else {
            return Ok(None);
        };
        let aberto = ctx
            .recurso::<Cofre>()?
            .abrir(&selado, &contexto_csc(ctx.empresa))
            .map_err(|e| Erro::de_dominio(&e))?;
        let token = String::from_utf8(aberto.expor().clone())
            .map_err(|_| Erro::novo(CodigoErro::FALHA_INTERNA, "CSC guardado ilegível"))?;
        Ok(Some(Csc {
            id,
            token: Segredo::novo(token),
        }))
    }

    /// Uma porta por ambiente presente na fila.
    type Portas = HashMap<AmbienteFiscal, Box<dyn PortaFiscal>>;

    /// Transmite a fila: emite os pendentes e consulta os incertos, um a um, fora de qualquer
    /// transação; grava cada resultado na hora. Para quando a API não responde (os outros
    /// esperam a próxima rodada). Devolve um resumo.
    ///
    /// # Errors
    /// A instalação não consegue falar com a API (sem certificado, sem provedor, cofre de
    /// outra instalação) — nesse caso nada muda na fila.
    pub(crate) fn transmitir_pendentes(t: &ContextoTarefa<'_>) -> Resultado<String> {
        let empresa = t.ctx.empresa;
        let Some(_guarda) = Guarda::tomar(empresa) else {
            return Ok("já há uma transmissão em andamento".into());
        };
        let fila = t.ler(|c| sqld::fila(c, empresa, Instante::agora(), 20))?;
        if fila.is_empty() {
            return Ok("nada na fila".into());
        }
        let ambientes: HashSet<AmbienteFiscal> = fila.iter().map(|d| d.ambiente).collect();
        let (portas, csc, uf): (Portas, Option<Csc>, Option<Uf>) = t.ler(|c| {
            let mut portas = HashMap::new();
            for a in ambientes {
                portas.insert(
                    a,
                    crate::provedor::porta_da_empresa_no_ambiente(&t.ctx, c, a)?,
                );
            }
            let csc = csc(&t.ctx, c)?;
            let uf = mod_empresa::sql::dados_fiscais(c, empresa)?.uf();
            Ok((portas, csc, uf))
        })?;
        let cnpj = t.ler(|c| {
            Ok(cardeal_kernel::Cnpj::novo(&mod_empresa::sql::dados(c, empresa)?.cnpj).ok())
        })?;
        let mut eventos = (0_u32, 0_u32, 0_u32); // cancelados, inutilizados, recusados
        let mut contagem: HashMap<EstadoDocumento, u32> = HashMap::new();
        for doc in fila {
            let Some(porta) = portas.get(&doc.ambiente) else {
                continue;
            };
            let desfecho = match doc.pedido {
                Some(pedido) => {
                    desfecho_do_pedido(porta.as_ref(), pedido, &doc, uf, cnpj, t.ctx.fuso)
                }
                None => desfecho_da_emissao(porta.as_ref(), &doc, csc.as_ref(), uf),
            };
            let parar = matches!(desfecho, Desfecho::Indisponivel(_));
            let mudanca = if let Some(pedido) = doc.pedido {
                let m = decidir_evento(pedido, doc.estado, doc.tentativas, desfecho);
                match m.estado {
                    EstadoDocumento::Cancelada => eventos.0 += 1,
                    EstadoDocumento::Inutilizada => eventos.1 += 1,
                    _ if m.limpar_pedido => eventos.2 += 1,
                    _ => {}
                }
                m
            } else {
                let m = decidir(doc.estado, doc.tentativas, desfecho);
                *contagem.entry(m.estado).or_default() += 1;
                m
            };
            // Venda desfeita: o documento que acaba de chegar a autorizado ou rejeitado já
            // recebe o pedido de cancelar ou inutilizar.
            let pedido_da_anulacao =
                (doc.anulada && doc.pedido.is_none() && mudanca.estado != doc.estado)
                    .then(|| crate::documento::pedido_da_anulacao(mudanca.estado, 0))
                    .flatten()
                    .filter(|_| mudanca.estado != EstadoDocumento::Pendente);
            let justificativa = doc.justificativa.clone().unwrap_or_default();
            let (id, estado_lido) = (doc.id, doc.estado);
            t.escrever(move |uow| {
                let agora = Instante::agora();
                let aplicou = sqld::aplicar_mudanca(
                    uow.conexao(),
                    empresa,
                    id,
                    estado_lido,
                    &mudanca,
                    agora,
                )?;
                if let (true, Some(p)) = (aplicou, pedido_da_anulacao) {
                    sqld::pedir(
                        uow.conexao(),
                        empresa,
                        id,
                        p,
                        &justificativa,
                        mudanca.estado,
                        agora,
                    )?;
                }
                Ok(aplicou)
            })?;
            if parar {
                break;
            }
        }
        let n = |e| contagem.get(&e).copied().unwrap_or(0);
        let mut resumo = format!(
            "{} autorizada(s), {} rejeitada(s), {} na fila",
            n(EstadoDocumento::Autorizada),
            n(EstadoDocumento::Rejeitada) + n(EstadoDocumento::Denegada),
            n(EstadoDocumento::Pendente) + n(EstadoDocumento::Incerta)
        );
        let (cancelados, inutilizados, recusados) = eventos;
        if cancelados + inutilizados + recusados > 0 {
            let _ = write!(
                resumo,
                "; {cancelados} cancelada(s), {inutilizados} inutilizada(s), {recusados} pedido(s) recusado(s)"
            );
        }
        Ok(resumo)
    }

    /// Emite (pendente) ou consulta (incerta) um documento.
    fn desfecho_da_emissao(
        porta: &dyn PortaFiscal,
        doc: &sqld::NaFila,
        csc: Option<&Csc>,
        uf: Option<Uf>,
    ) -> Desfecho {
        match (doc.estado, &doc.nota, &doc.chave, csc, uf) {
            (EstadoDocumento::Pendente, Some(blob), _, Some(csc), _) => {
                match postcard::from_bytes::<Nfce>(blob) {
                    Ok(nota) => desfecho(porta.emitir_nfce(&nota, csc)),
                    Err(e) => Desfecho::Recusado(format!("a nota guardada está ilegível: {e}")),
                }
            }
            (EstadoDocumento::Pendente, _, _, None, _) => Desfecho::Recusado(
                "o CSC da NFC-e não está cadastrado (Configurações › Fiscal)".into(),
            ),
            (EstadoDocumento::Incerta, _, Some(chave), _, Some(uf)) => {
                match ChaveAcesso::nova(chave) {
                    Ok(chave) => desfecho(porta.consultar_documento(&chave, uf)),
                    Err(e) => Desfecho::Recusado(e.to_string()),
                }
            }
            _ => Desfecho::Recusado("documento sem nota ou chave para transmitir".into()),
        }
    }

    /// Leva um pedido (cancelar/inutilizar) à porta.
    fn desfecho_do_pedido(
        porta: &dyn PortaFiscal,
        pedido: PedidoDocumento,
        doc: &sqld::NaFila,
        uf: Option<Uf>,
        cnpj: Option<cardeal_kernel::Cnpj>,
        fuso: cardeal_kernel::Fuso,
    ) -> Desfecho {
        let (Some(justificativa), Some(uf)) = (doc.justificativa.as_deref(), uf) else {
            return Desfecho::Recusado("faltam a justificativa ou a UF da empresa".into());
        };
        match pedido {
            PedidoDocumento::Cancelar => {
                let (Some(chave), Some(protocolo)) =
                    (doc.chave.as_deref(), doc.protocolo.as_deref())
                else {
                    return Desfecho::Recusado(
                        "a nota não tem chave e protocolo para cancelar".into(),
                    );
                };
                match ChaveAcesso::nova(chave) {
                    Ok(chave) => desfecho(porta.cancelar(&chave, protocolo, justificativa, uf)),
                    Err(e) => Desfecho::Recusado(e.to_string()),
                }
            }
            PedidoDocumento::Inutilizar => {
                let Some(cnpj) = cnpj else {
                    return Desfecho::Recusado("o CNPJ da empresa é inválido".into());
                };
                let ano =
                    u16::try_from(doc.criado_em.data(fuso).ano().rem_euclid(100)).unwrap_or(0);
                desfecho(porta.inutilizar(&cardeal_fiscal::PedidoInutilizacao {
                    cnpj,
                    modelo: MODELO,
                    serie: doc.serie,
                    inicio: doc.numero,
                    fim: doc.numero,
                    ano,
                    justificativa: justificativa.to_string(),
                    uf,
                }))
            }
        }
    }

    fn desfecho(r: Resultado<cardeal_fiscal::RetornoSefaz>) -> Desfecho {
        match r {
            Ok(r) => Desfecho::Resposta(r),
            Err(e) if e.codigo == CodigoErro::FISCAL_INDISPONIVEL => {
                Desfecho::Indisponivel(e.mensagem)
            }
            Err(e) => Desfecho::Recusado(e.mensagem),
        }
    }
}
