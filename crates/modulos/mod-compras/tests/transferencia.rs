//! A transferência de mercadoria entre filiais do grupo, pelo despacho de verdade: a matriz
//! envia (estoque ao custo médio, Razão, NF-e de transferência pela fila contra o
//! `FiscalSimulado`), a filial recebe só com a NF-e autorizada, e o envio sem nota autorizada
//! se cancela devolvendo a mercadoria. As três empresas na mesma base (a organização).

#![allow(clippy::result_large_err)]

use std::sync::Arc;

use cardeal_auth::{AutorizacoesEfetivas, EmissaoSessao, Escopo, Papel, Sessao};
use cardeal_fiscal::FiscalSimulado;
use cardeal_kernel::{Dinheiro, Erro, Id, Instante, Preco, Quantidade};
use cardeal_ledger::semear_plano_padrao;
use cardeal_modkit::{Ambiente, Despachante, Modulo, PedidoAtivacao, Recursos, RegistroModulos};
use cardeal_storage::{
    Armazenamento, Cofre, ConfigArmazenamento, ContextoEscrita, ErroArmazenamento,
};
use mod_compras::{
    CancelarTransferencia, EmitirNfeDaTransferencia, EnviarTransferencia, EstadoTransferencia,
    ItemATransferir, ItemDaTransferencia, ItensDaTransferencia, ModuloCompras,
    ReceberTransferencia, SituacaoDaNota, TransferenciaEntreFiliais, TransferenciaEnviada,
    TransferenciasEntreFiliais,
};
use mod_empresa::{DadosFiscais, DefinirDadosFiscaisEmpresa, EnderecoFiscal, ModuloEmpresa};
use mod_estoque::{
    CriarGrupoProduto, CriarLocal, CriarProduto, CriarUnidade, GrupoProdutoCriado, LocalCriado,
    ProdutoCriado, RegistrarEntrada, TipoLocal, UnidadeCriada,
};
use mod_fiscal::{InstalarCertificado, ProvedorFiscal, TAREFA_TRANSMITIR};
use serde::de::DeserializeOwned;
use serde::Serialize;
use tempfile::TempDir;

const PERMISSOES: &[&str] = &[
    "empresa.dados.ver",
    "empresa.dados.editar",
    "fiscal.ver",
    "fiscal.emitir",
    "fiscal.configurar",
    "estoque.produto.criar",
    "estoque.local.criar",
    "estoque.movimento.entrada",
    "compras.transferencia.ver",
    "compras.transferencia.enviar",
    "compras.transferencia.receber",
];

/// Uma empresa do grupo: a sessão e o ambiente dela.
struct Empresa {
    id: Id,
    sessao: Sessao,
    amb: Ambiente,
}

struct Mundo {
    _dir: TempDir,
    arm: Armazenamento,
    desp: Despachante,
    matriz: Empresa,
    filial: Empresa,
    outra: Empresa,
}

impl Mundo {
    fn novo() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let arm =
            Armazenamento::abrir(ConfigArmazenamento::arquivo(dir.path().join("c.db"))).unwrap();
        arm.migrar(&[
            cardeal_ledger::migracoes::conjunto(),
            ModuloEmpresa.migracoes(),
            mod_clientes::ModuloClientes.migracoes(),
            mod_estoque::ModuloEstoque.migracoes(),
            mod_financeiro::ModuloFinanceiro.migracoes(),
            ModuloCompras.migracoes(),
            mod_fiscal::ModuloFiscal.migracoes(),
        ])
        .unwrap();
        let (matriz, filial, outra) = (Id::novo(), Id::novo(), Id::novo());
        for (id, razao, cnpj) in [
            (matriz, "Loja Matriz LTDA", "11222333000181"),
            (filial, "Loja Matriz LTDA", "11222333000262"),
            (outra, "Outra Empresa LTDA", "45543915000181"),
        ] {
            arm.escritor()
                .executar(
                    ContextoEscrita::novo(id, Id::novo(), Id::novo(), Id::novo()),
                    move |uow| {
                        uow.conexao()
                            .execute(
                                "INSERT INTO nucleo_empresa
                                   (id, razao_social, nome_fantasia, cnpj, regime, endereco,
                                    perfil, criado_em)
                                 VALUES (?1, ?2, ?2, ?3, 'SimplesNacional', '{}', 'comercio', 0)",
                                rusqlite::params![id.em_bytes().as_slice(), razao, cnpj],
                            )
                            .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))?;
                        semear_plano_padrao(uow, id)
                            .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))
                    },
                )
                .unwrap();
        }
        // A filial é de uma base de antes da conta corrente do grupo: ela nasce na transferência.
        arm.escritor()
            .executar(
                ContextoEscrita::novo(filial, Id::novo(), Id::novo(), Id::novo()),
                move |uow| {
                    uow.conexao()
                        .execute(
                            "DELETE FROM razao_conta
                             WHERE empresa = ?1 AND papel_padrao = 'ContaCorrenteGrupo'",
                            [filial.em_bytes().as_slice()],
                        )
                        .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))
                },
            )
            .unwrap();
        let mut rm = RegistroModulos::novo();
        for m in [
            &mod_empresa::MANIFESTO,
            &mod_clientes::MANIFESTO,
            &mod_estoque::MANIFESTO,
            &mod_financeiro::MANIFESTO,
            &mod_compras::MANIFESTO,
            &mod_fiscal::MANIFESTO,
        ] {
            rm.registrar(m).unwrap();
        }
        let pedido = PedidoAtivacao::nova()
            .com_modulo("empresa")
            .com_modulo("compras")
            .com_modulo("fiscal");
        let sim = Arc::new(FiscalSimulado::novo());
        let recursos = Arc::new(
            Recursos::novo()
                .com(Arc::new(Cofre::aleatorio()))
                .com(Arc::new(ProvedorFiscal::simulado(Arc::clone(&sim)))),
        );
        let empresa = |id: Id| {
            let mut papel = Papel::novo(id, "Dono");
            for p in PERMISSOES {
                papel = papel.com_permissao(*p);
            }
            Empresa {
                id,
                sessao: Sessao::abrir(EmissaoSessao::padrao(
                    Id::novo(),
                    Id::novo(),
                    Escopo::empresa_inteira(id),
                    AutorizacoesEfetivas::consolidar([&papel]),
                    Instante::EPOCA,
                )),
                amb: Ambiente::novo(id, rm.resolver(&pedido).unwrap())
                    .com_organizacao(matriz)
                    .com_recursos(Arc::clone(&recursos)),
            }
        };
        let desp = Despachante::construir(&[
            &ModuloEmpresa,
            &mod_clientes::ModuloClientes,
            &mod_estoque::ModuloEstoque,
            &mod_financeiro::ModuloFinanceiro,
            &ModuloCompras,
            &mod_fiscal::ModuloFiscal,
        ])
        .unwrap();
        let m = Self {
            _dir: dir,
            desp,
            matriz: empresa(matriz),
            filial: empresa(filial),
            outra: empresa(outra),
            arm,
        };
        for e in [&m.matriz, &m.filial, &m.outra] {
            let _: DadosFiscais = m
                .executar(
                    e,
                    "empresa.definir_dados_fiscais.v1",
                    &DefinirDadosFiscaisEmpresa {
                        dados: DadosFiscais {
                            inscricao_estadual: "0960000000".into(),
                            inscricao_municipal: String::new(),
                            endereco: EnderecoFiscal {
                                logradouro: "Rua A".into(),
                                numero: "1".into(),
                                complemento: String::new(),
                                bairro: "Centro".into(),
                                municipio: "Porto Alegre".into(),
                                codigo_municipio: "4314902".into(),
                                uf: "RS".into(),
                                cep: "90010000".into(),
                            },
                        },
                    },
                )
                .unwrap();
        }
        let pfx = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../cardeal-fiscal/tests/dados/aes256.pfx"),
        )
        .unwrap();
        let _: mod_fiscal::CertificadoResumo = m
            .executar(
                &m.matriz,
                "fiscal.instalar_certificado.v1",
                &InstalarCertificado {
                    arquivo: pfx,
                    senha: "cardeal-teste".into(),
                },
            )
            .unwrap();
        m
    }

    fn executar<S: DeserializeOwned>(
        &self,
        e: &Empresa,
        nome: &str,
        cmd: &impl Serialize,
    ) -> Result<S, Erro> {
        let saida = self.desp.executar_comando(
            nome,
            &postcard::to_stdvec(cmd).unwrap(),
            &e.sessao,
            &e.amb,
            self.arm.escritor(),
        )?;
        Ok(postcard::from_bytes(&saida).unwrap())
    }

    fn consultar<S: DeserializeOwned>(&self, e: &Empresa, nome: &str, q: &impl Serialize) -> S {
        let saida = self
            .desp
            .executar_consulta(
                nome,
                &postcard::to_stdvec(q).unwrap(),
                &e.sessao,
                &e.amb,
                self.arm.leitor(),
            )
            .unwrap();
        postcard::from_bytes(&saida).unwrap()
    }

    fn transmitir(&self) -> String {
        self.desp
            .executar_tarefa(
                TAREFA_TRANSMITIR,
                &self.matriz.sessao,
                &self.matriz.amb,
                self.arm.leitor(),
                self.arm.escritor(),
            )
            .unwrap()
    }

    fn local(&self, e: &Empresa, nome: &str) -> Id {
        let l: LocalCriado = self
            .executar(
                e,
                "estoque.criar_local.v1",
                &CriarLocal {
                    nome: nome.into(),
                    tipo: TipoLocal::Deposito,
                },
            )
            .unwrap();
        l.local
    }

    /// O saldo e o custo médio do produto no local.
    fn saldo(&self, e: &Empresa, produto: Id, local: Id) -> (Quantidade, Preco) {
        let (empresa, produto, local) = (e.id, produto, local);
        self.arm
            .leitor()
            .consultar(move |c| {
                Ok(c.query_row(
                    "SELECT quantidade_disponivel, custo_medio FROM estoque_saldo_local
                     WHERE empresa = ?1 AND produto = ?2 AND local = ?3",
                    rusqlite::params![
                        empresa.em_bytes().as_slice(),
                        produto.em_bytes().as_slice(),
                        local.em_bytes().as_slice()
                    ],
                    |r| Ok((Quantidade::interna(r.get(0)?), Preco::interna(r.get(1)?))),
                )
                .unwrap_or((Quantidade::ZERO, Preco::ZERO)))
            })
            .unwrap()
    }

    /// Quantos movimentos de estoque a transferência gerou na empresa, por tipo.
    fn movimentos(&self, e: &Empresa, tipo: &'static str) -> i64 {
        let empresa = e.id;
        self.arm
            .leitor()
            .consultar(move |c| {
                c.query_row(
                    "SELECT COUNT(*) FROM estoque_movimento
                     WHERE empresa = ?1 AND origem_modulo = 'compras_transferencia' AND tipo = ?2",
                    rusqlite::params![empresa.em_bytes().as_slice(), tipo],
                    |r| r.get(0),
                )
                .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))
            })
            .unwrap()
    }

    /// O saldo de uma conta (pelo papel) da empresa no Razão.
    fn saldo_do_papel(&self, e: &Empresa, papel: &'static str) -> Dinheiro {
        let empresa = e.id;
        self.arm
            .leitor()
            .consultar(move |c| {
                c.query_row(
                    "SELECT COALESCE(SUM(p.valor), 0) FROM razao_partida p
                     JOIN razao_conta k ON k.id = p.conta
                     WHERE k.empresa = ?1 AND k.papel_padrao = ?2",
                    rusqlite::params![empresa.em_bytes().as_slice(), papel],
                    |r| r.get::<_, i64>(0),
                )
                .map(Dinheiro::centavos)
                .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))
            })
            .unwrap()
    }
}

#[test]
#[allow(clippy::too_many_lines)]
fn a_matriz_envia_com_nfe_a_filial_recebe_autorizada_e_o_envio_sem_nota_se_cancela() {
    let m = Mundo::novo();
    // O catálogo é do grupo: o produto criado na matriz é o mesmo na filial.
    let grupo: GrupoProdutoCriado = m
        .executar(
            &m.matriz,
            "estoque.criar_grupo_produto.v1",
            &CriarGrupoProduto {
                codigo: "GERAL".into(),
                nome: "Geral".into(),
                pai: None,
            },
        )
        .unwrap();
    let unidade: UnidadeCriada = m
        .executar(
            &m.matriz,
            "estoque.criar_unidade.v1",
            &CriarUnidade {
                sigla: "UN".into(),
                nome: "Unidade".into(),
                fracionavel: false,
            },
        )
        .unwrap();
    let produto = |nome: &str, ncm: &str| -> Id {
        let p: ProdutoCriado = m
            .executar(
                &m.matriz,
                "estoque.criar_produto.v1",
                &CriarProduto {
                    grupo_produto: grupo.grupo_produto,
                    nome: nome.into(),
                    ncm: ncm.into(),
                    unidade_padrao: unidade.unidade,
                    codigo_barras: None,
                    detalhes_tecnicos: None,
                },
            )
            .unwrap();
        p.produto
    };
    let capinha = produto("Capinha de celular", "39269090");
    let brinde = produto("Brinde", "");
    let loja = m.local(&m.matriz, "Loja");
    let deposito = m.local(&m.filial, "Depósito da filial");
    for p in [capinha, brinde] {
        let _: mod_estoque::EntradaRegistrada = m
            .executar(
                &m.matriz,
                "estoque.registrar_entrada.v1",
                &RegistrarEntrada {
                    produto: p,
                    local: loja,
                    quantidade: Quantidade::unidades(50),
                    custo_unitario: Preco::reais(10),
                },
            )
            .unwrap();
    }
    let enviar = |destino: Id, produto: Id, q: i64| {
        m.executar::<TransferenciaEnviada>(
            &m.matriz,
            "compras.enviar_transferencia.v1",
            &EnviarTransferencia {
                destino,
                local: loja,
                itens: vec![ItemATransferir {
                    produto,
                    quantidade: Quantidade::unidades(q),
                }],
                observacao: "  reposição   da filial ".into(),
            },
        )
    };

    // O que não sai: outra raiz de CNPJ, mais do que o local tem, para si mesma.
    let erro = enviar(m.outra.id, capinha, 5).unwrap_err();
    assert!(erro.mensagem.contains("por venda"), "{}", erro.mensagem);
    let erro = enviar(m.filial.id, capinha, 51).unwrap_err();
    assert!(
        erro.mensagem.contains("o local tem 50"),
        "{}",
        erro.mensagem
    );
    let erro = enviar(m.matriz.id, capinha, 5).unwrap_err();
    assert!(erro.mensagem.contains("outro CNPJ"), "{}", erro.mensagem);
    assert_eq!(
        m.saldo(&m.matriz, capinha, loja).0,
        Quantidade::unidades(50)
    );

    // A matriz envia 20: sai ao custo, o Razão lança e a NF-e vai para a fila.
    let enviada = enviar(m.filial.id, capinha, 20).unwrap();
    assert_eq!(enviada.valor, Dinheiro::reais(200));
    assert_eq!(enviada.nota, SituacaoDaNota::NaFila);
    assert_eq!(
        m.saldo(&m.matriz, capinha, loja).0,
        Quantidade::unidades(30)
    );
    assert_eq!(
        m.saldo_do_papel(&m.matriz, "ContaCorrenteGrupo"),
        Dinheiro::reais(200)
    );

    // A filial vê a caminho, mas só recebe com a NF-e autorizada; a matriz não cancela com a
    // nota na fila.
    let a_caminho: Vec<TransferenciaEntreFiliais> = m.consultar(
        &m.filial,
        "compras.transferencias.v1",
        &TransferenciasEntreFiliais { so_a_caminho: true },
    );
    assert_eq!(a_caminho.len(), 1);
    assert!(!a_caminho[0].enviada_por_esta);
    assert_eq!(a_caminho[0].origem_nome, "Loja Matriz LTDA");
    assert_eq!(
        a_caminho[0].observacao.as_deref(),
        Some("reposição da filial")
    );
    let receber = |t: Id| {
        m.executar::<Dinheiro>(
            &m.filial,
            "compras.receber_transferencia.v1",
            &ReceberTransferencia {
                transferencia: t,
                local: deposito,
            },
        )
    };
    let erro = receber(enviada.transferencia).unwrap_err();
    assert!(
        erro.mensagem.contains("ainda não está autorizada"),
        "{}",
        erro.mensagem
    );
    let cancelar = |t: Id| {
        m.executar::<()>(
            &m.matriz,
            "compras.cancelar_transferencia.v1",
            &CancelarTransferencia {
                transferencia: t,
                motivo: "mandei o produto errado".into(),
            },
        )
    };
    assert!(cancelar(enviada.transferencia)
        .unwrap_err()
        .mensagem
        .contains("na fila"));

    // A fila transmite; autorizada, a matriz já não cancela e a filial recebe ao mesmo custo.
    assert!(m.transmitir().contains("1 autorizada"));
    assert!(cancelar(enviada.transferencia)
        .unwrap_err()
        .mensagem
        .contains("Notas fiscais"));
    assert_eq!(
        receber(enviada.transferencia).unwrap(),
        Dinheiro::reais(200)
    );
    assert_eq!(
        m.saldo(&m.filial, capinha, deposito),
        (Quantidade::unidades(20), Preco::reais(10))
    );
    assert_eq!(
        m.saldo_do_papel(&m.filial, "ContaCorrenteGrupo"),
        Dinheiro::reais(-200),
        "a conta nasceu na filial e, somada no grupo, zera"
    );
    assert_eq!(
        m.saldo_do_papel(&m.filial, "EstoqueMercadorias"),
        Dinheiro::reais(200)
    );
    assert!(receber(enviada.transferencia)
        .unwrap_err()
        .mensagem
        .contains("já foi recebida"));
    let itens: Vec<ItemDaTransferencia> = m.consultar(
        &m.filial,
        "compras.itens_da_transferencia.v1",
        &ItensDaTransferencia {
            transferencia: enviada.transferencia,
        },
    );
    assert_eq!(itens.len(), 1);
    assert_eq!(itens[0].nome, "Capinha de celular");
    let todas: Vec<TransferenciaEntreFiliais> = m.consultar(
        &m.matriz,
        "compras.transferencias.v1",
        &TransferenciasEntreFiliais {
            so_a_caminho: false,
        },
    );
    assert_eq!(todas[0].estado, EstadoTransferencia::Recebida);
    assert_eq!(todas[0].nota, SituacaoDaNota::Autorizada);
    assert!(todas[0].numero_nota.is_some());

    // O brinde sem NCM: a nota sai rejeitada já, com o porquê; emitida de novo, continua; o
    // envio se cancela e tudo volta.
    let sem_ncm = enviar(m.filial.id, brinde, 5).unwrap();
    assert!(
        matches!(&sem_ncm.nota, SituacaoDaNota::Rejeitada(p) if p.contains("sem NCM")),
        "{:?}",
        sem_ncm.nota
    );
    let de_novo: SituacaoDaNota = m
        .executar(
            &m.matriz,
            "compras.emitir_nfe_da_transferencia.v1",
            &EmitirNfeDaTransferencia {
                transferencia: sem_ncm.transferencia,
            },
        )
        .unwrap();
    assert!(matches!(de_novo, SituacaoDaNota::Rejeitada(_)));
    assert_eq!(m.saldo(&m.matriz, brinde, loja).0, Quantidade::unidades(45));
    cancelar(sem_ncm.transferencia).unwrap();
    assert_eq!(
        m.saldo(&m.matriz, brinde, loja),
        (Quantidade::unidades(50), Preco::reais(10))
    );
    assert_eq!(
        m.saldo_do_papel(&m.matriz, "ContaCorrenteGrupo"),
        Dinheiro::reais(200),
        "o estorno desfez o envio cancelado"
    );
    assert!(receber(sem_ncm.transferencia)
        .unwrap_err()
        .mensagem
        .contains("cancelada"));
    let a_caminho: Vec<TransferenciaEntreFiliais> = m.consultar(
        &m.filial,
        "compras.transferencias.v1",
        &TransferenciasEntreFiliais { so_a_caminho: true },
    );
    assert!(a_caminho.is_empty());

    // Transferência não é consumo nem compra: os movimentos são de transferência (o ABC, o
    // giro e o histórico de compras leem só Saida/Entrada).
    assert_eq!(m.movimentos(&m.matriz, "TransferenciaSaida"), 2);
    assert_eq!(
        m.movimentos(&m.matriz, "TransferenciaEntrada"),
        1,
        "o cancelado voltou"
    );
    assert_eq!(m.movimentos(&m.filial, "TransferenciaEntrada"), 1);
    for e in [&m.matriz, &m.filial] {
        assert_eq!(m.movimentos(e, "Saida") + m.movimentos(e, "Entrada"), 0);
    }
}
