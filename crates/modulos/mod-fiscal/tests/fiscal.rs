//! O fiscal atravessando o `Despachante` real contra SQLite: certificado conferido e guardado
//! cifrado no cofre, ambiente, pendências e o status da SEFAZ pelo provedor (simulado).

#![allow(clippy::result_large_err)]

use std::sync::Arc;

use cardeal_auth::{AutorizacoesEfetivas, EmissaoSessao, Escopo, Papel, Sessao};
use cardeal_fiscal::FiscalSimulado;
use cardeal_kernel::{CodigoErro, Erro, Id, Instante};
use cardeal_modkit::{Ambiente, Despachante, Modulo, PedidoAtivacao, Recursos, RegistroModulos};
use cardeal_storage::{
    Armazenamento, Cofre, ConfigArmazenamento, ContextoEscrita, ErroArmazenamento,
};
use mod_empresa::{DadosFiscais, DefinirDadosFiscaisEmpresa, EnderecoFiscal, ModuloEmpresa};
use mod_fiscal::{
    AmbienteFiscal, CertificadoResumo, DefinirAmbienteFiscal, InstalarCertificado, ModeloFiscal,
    ModuloFiscal, PendenciaFiscal, ProvedorFiscal, RemoverCertificado, SituacaoFiscal,
    SituacaoFiscalDaEmpresa, StatusDaSefaz, StatusSefaz,
};
use serde::de::DeserializeOwned;
use serde::Serialize;
use tempfile::TempDir;

const SENHA: &str = "cardeal-teste";

fn pfx(nome: &str) -> Vec<u8> {
    let caminho = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../cardeal-fiscal/tests/dados")
        .join(nome);
    std::fs::read(&caminho).unwrap_or_else(|e| panic!("{}: {e}", caminho.display()))
}

struct Mundo {
    _dir: TempDir,
    arm: Armazenamento,
    desp: Despachante,
    ambiente: Ambiente,
    sessao: Sessao,
    simulado: Arc<FiscalSimulado>,
    empresa: Id,
}

impl Mundo {
    fn novo() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let arm =
            Armazenamento::abrir(ConfigArmazenamento::arquivo(dir.path().join("c.db"))).unwrap();
        arm.migrar(&[ModuloEmpresa.migracoes(), ModuloFiscal.migracoes()])
            .unwrap();
        let empresa = Id::novo();
        arm.escritor()
            .executar(ContextoEscrita::novo(empresa, Id::novo(), Id::novo(), Id::novo()), move |uow| {
                uow.conexao()
                    .execute(
                        "INSERT INTO nucleo_empresa
                           (id, razao_social, nome_fantasia, cnpj, regime, endereco, perfil, criado_em)
                         VALUES (?1,'Empresa Teste LTDA','Teste','11222333000181','SimplesNacional','{}','comercio',0)",
                        [empresa.em_bytes().as_slice()],
                    )
                    .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))
            })
            .unwrap();

        let mut rm = RegistroModulos::novo();
        rm.registrar(&mod_empresa::MANIFESTO).unwrap();
        rm.registrar(&mod_fiscal::MANIFESTO).unwrap();
        let efetivo = rm
            .resolver(
                &PedidoAtivacao::nova()
                    .com_modulo("empresa")
                    .com_modulo("fiscal"),
            )
            .unwrap();
        let simulado = Arc::new(FiscalSimulado::novo());
        let recursos = Recursos::novo()
            .com(Arc::new(Cofre::aleatorio()))
            .com(Arc::new(ProvedorFiscal::simulado(Arc::clone(&simulado))));
        let ambiente = Ambiente::novo(empresa, efetivo).com_recursos(Arc::new(recursos));

        let mut papel = Papel::novo(empresa, "Admin");
        for p in [
            "fiscal.ver",
            "fiscal.configurar",
            "empresa.dados.ver",
            "empresa.dados.editar",
        ] {
            papel = papel.com_permissao(p);
        }
        let sessao = Sessao::abrir(EmissaoSessao::padrao(
            Id::novo(),
            Id::novo(),
            Escopo::empresa_inteira(empresa),
            AutorizacoesEfetivas::consolidar([&papel]),
            Instante::EPOCA,
        ));
        let desp = Despachante::construir(&[&ModuloEmpresa, &ModuloFiscal]).unwrap();
        Self {
            _dir: dir,
            arm,
            desp,
            ambiente,
            sessao,
            simulado,
            empresa,
        }
    }

    fn executar<S: DeserializeOwned>(&self, nome: &str, cmd: &impl Serialize) -> Result<S, Erro> {
        let saida = self.desp.executar_comando(
            nome,
            &postcard::to_stdvec(cmd).unwrap(),
            &self.sessao,
            &self.ambiente,
            self.arm.escritor(),
        )?;
        Ok(postcard::from_bytes(&saida).unwrap())
    }

    fn consultar<S: DeserializeOwned>(&self, nome: &str, q: &impl Serialize) -> Result<S, Erro> {
        let saida = self.desp.executar_consulta(
            nome,
            &postcard::to_stdvec(q).unwrap(),
            &self.sessao,
            &self.ambiente,
            self.arm.leitor(),
        )?;
        Ok(postcard::from_bytes(&saida).unwrap())
    }

    fn instalar(&self, arquivo: &str, senha: &str) -> Result<CertificadoResumo, Erro> {
        self.executar(
            "fiscal.instalar_certificado.v1",
            &InstalarCertificado {
                arquivo: pfx(arquivo),
                senha: senha.into(),
            },
        )
    }

    fn situacao(&self) -> SituacaoFiscalDaEmpresa {
        self.consultar("fiscal.situacao.v1", &SituacaoFiscal)
            .unwrap()
    }

    fn status(&self) -> Result<StatusSefaz, Erro> {
        self.consultar(
            "fiscal.status_sefaz.v1",
            &StatusDaSefaz {
                modelo: ModeloFiscal::Nfce,
            },
        )
    }

    fn com_recursos(&self, recursos: Recursos) -> Ambiente {
        self.ambiente.clone().com_recursos(Arc::new(recursos))
    }
}

fn dados_fiscais() -> DadosFiscais {
    DadosFiscais {
        inscricao_estadual: "255123456".into(),
        inscricao_municipal: String::new(),
        endereco: EnderecoFiscal {
            logradouro: "Rua das Flores".into(),
            numero: "100".into(),
            complemento: String::new(),
            bairro: "Centro".into(),
            municipio: "Piratuba".into(),
            codigo_municipio: "4213104".into(),
            uf: "SC".into(),
            cep: "89667000".into(),
        },
    }
}

#[test]
fn do_certificado_ao_status_da_sefaz() {
    let m = Mundo::novo();

    // Empresa nova: sem certificado, sem dados fiscais, em homologação.
    let s = m.situacao();
    assert_eq!(s.ambiente, AmbienteFiscal::Homologacao);
    assert_eq!(
        s.pendencias,
        vec![
            PendenciaFiscal::SemCertificado,
            PendenciaFiscal::DadosFiscaisIncompletos
        ]
    );
    assert!(!s.pronta_para_emitir());
    assert_eq!(m.status().unwrap_err().codigo, CodigoErro::REGRA_VIOLADA); // dados incompletos

    let _: DadosFiscais = m
        .executar(
            "empresa.definir_dados_fiscais.v1",
            &DefinirDadosFiscaisEmpresa {
                dados: dados_fiscais(),
            },
        )
        .unwrap();
    assert_eq!(
        m.situacao().pendencias,
        vec![PendenciaFiscal::SemCertificado]
    );
    // Sem certificado não há porta.
    assert!(m.status().unwrap_err().mensagem.contains("certificado"));

    // Recusas: senha errada, vencido, de pessoa física.
    let senha_errada = m.instalar("aes256.pfx", "errada").unwrap_err();
    assert_eq!(senha_errada.codigo, CodigoErro::ENTRADA_INVALIDA);
    assert!(
        senha_errada.mensagem.contains("senha"),
        "{}",
        senha_errada.mensagem
    );
    assert!(m
        .instalar("vencido.pfx", SENHA)
        .unwrap_err()
        .mensagem
        .contains("venceu"));
    assert!(m
        .instalar("pessoa.pfx", SENHA)
        .unwrap_err()
        .mensagem
        .contains("e-CNPJ"));
    assert!(m.situacao().certificado.is_none());

    // O de RC2 instala, regravado.
    let resumo = m.instalar("legado.pfx", SENHA).unwrap();
    assert_eq!(resumo.titular, "EMPRESA TESTE LTDA");
    assert_eq!(resumo.cnpj, "11.222.333/0001-81");
    assert!(resumo.convertido);
    let s = m.situacao();
    assert_eq!(s.certificado.as_ref(), Some(&resumo));
    assert!(s.pendencias.is_empty());
    assert!(s.pronta_para_emitir());

    // A base guarda só texto cifrado: nem a senha nem o arquivo aparecem.
    let selado: Vec<u8> = m
        .arm
        .leitor()
        .consultar(|c| {
            c.query_row("SELECT selado FROM fiscal_certificado", [], |r| r.get(0))
                .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))
        })
        .unwrap();
    assert!(!selado.windows(SENHA.len()).any(|w| w == SENHA.as_bytes()));
    assert!(!selado
        .windows(16)
        .any(|w| pfx("legado.pfx").windows(16).any(|p| p == w)));

    // O status passa pelo provedor com o certificado do cofre.
    let status = m.status().unwrap();
    assert!(status.em_operacao);
    assert_eq!(status.codigo, "107");
    m.simulado.simular_fora_do_ar(true);
    assert_eq!(
        m.status().unwrap_err().codigo,
        CodigoErro::FISCAL_INDISPONIVEL
    );
    m.simulado.simular_fora_do_ar(false);

    // Trocar pelo de AES: substitui.
    let novo = m.instalar("aes256.pfx", SENHA).unwrap();
    assert!(!novo.convertido);
    assert_eq!(m.situacao().certificado, Some(novo));

    // Produção exige certificado; remover volta a pendência.
    let () = m
        .executar(
            "fiscal.definir_ambiente.v1",
            &DefinirAmbienteFiscal {
                ambiente: AmbienteFiscal::Producao,
            },
        )
        .unwrap();
    assert_eq!(m.situacao().ambiente, AmbienteFiscal::Producao);
    let removeu: bool = m
        .executar("fiscal.remover_certificado.v1", &RemoverCertificado)
        .unwrap();
    assert!(removeu);
    assert_eq!(
        m.situacao().pendencias,
        vec![PendenciaFiscal::SemCertificado]
    );
    let erro = m
        .executar::<()>(
            "fiscal.definir_ambiente.v1",
            &DefinirAmbienteFiscal {
                ambiente: AmbienteFiscal::Producao,
            },
        )
        .unwrap_err();
    assert!(erro.mensagem.contains("certificado"));
}

#[test]
fn cnpj_de_outra_empresa_e_recusado() {
    let m = Mundo::novo();
    let empresa = m.empresa;
    m.arm
        .escritor()
        .executar(
            ContextoEscrita::novo(empresa, Id::novo(), Id::novo(), Id::novo()),
            move |uow| {
                uow.conexao()
                    .execute(
                        "UPDATE nucleo_empresa SET cnpj = '11444777000161' WHERE id = ?1",
                        [empresa.em_bytes().as_slice()],
                    )
                    .map(|_| ())
                    .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))
            },
        )
        .unwrap();
    let erro = m.instalar("aes256.pfx", SENHA).unwrap_err();
    assert!(
        erro.mensagem.contains("11.222.333/0001-81")
            && erro.mensagem.contains("11.444.777/0001-61"),
        "{}",
        erro.mensagem
    );
}

#[test]
fn recursos_ausentes_ou_de_outra_instalacao() {
    let mut m = Mundo::novo();
    let _: DadosFiscais = m
        .executar(
            "empresa.definir_dados_fiscais.v1",
            &DefinirDadosFiscaisEmpresa {
                dados: dados_fiscais(),
            },
        )
        .unwrap();
    m.instalar("aes256.pfx", SENHA).unwrap();

    // Outra instalação (outra chave do cofre): o certificado não abre — e a mensagem manda
    // instalar de novo.
    let simulado = Arc::clone(&m.simulado);
    m.ambiente = m.com_recursos(
        Recursos::novo()
            .com(Arc::new(Cofre::aleatorio()))
            .com(Arc::new(ProvedorFiscal::simulado(simulado))),
    );
    let erro = m.status().unwrap_err();
    assert!(
        erro.mensagem.contains("outra instalação"),
        "{}",
        erro.mensagem
    );

    // Sem chave da API.
    let cofre_original = Arc::new(Cofre::aleatorio());
    m.ambiente = m.com_recursos(
        Recursos::novo()
            .com(Arc::clone(&cofre_original))
            .com(Arc::new(ProvedorFiscal::nao_configurado())),
    );
    m.instalar("aes256.pfx", SENHA).unwrap();
    let erro = m.status().unwrap_err();
    assert_eq!(erro.codigo, CodigoErro::ESTADO_INVALIDO);
    assert!(erro.mensagem.contains("CARDEAL_FISCAL_TOKEN"));

    // Sem cofre: não instala.
    m.ambiente = m.com_recursos(Recursos::novo());
    assert_eq!(
        m.instalar("aes256.pfx", SENHA).unwrap_err().codigo,
        CodigoErro::ESTADO_INVALIDO
    );
}
