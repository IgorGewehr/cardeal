//! Os comandos de clientes atravessando o `Despachante` real contra SQLite: autorização,
//! transação e persistência — tudo junto.

#![allow(clippy::result_large_err)] // `ErroArmazenamento` carrega detalhes de propósito

use cardeal_auth::{AutorizacoesEfetivas, EmissaoSessao, Escopo, Papel as PapelAuth, Sessao};
use cardeal_kernel::{CodigoErro, Dinheiro, Id, Instante};
use cardeal_modkit::{Ambiente, Despachante, Modulo, PedidoAtivacao, RegistroModulos};
use cardeal_storage::{Armazenamento, ConfigArmazenamento, ContextoEscrita, ErroArmazenamento};
use mod_clientes::{
    AdicionarContato, AdicionarEndereco, AdicionarPapel, ContatoFoiAdicionado, CriarPessoa,
    DefinirLimiteCredito, DetalhePessoa, EditarPessoa, EnderecoFoiAdicionado, ItemPessoa,
    LimiteCreditoDefinido, ModuloClientes, Papel, PapelFoiAdicionado, PessoaCadastrada,
    PessoaDetalhada, PessoaEditada, PessoasPorPapel, TipoContato, TipoDocumento, TipoEndereco,
    TipoPessoa, MANIFESTO,
};
use tempfile::TempDir;

fn base() -> (TempDir, Armazenamento, Id) {
    let dir = tempfile::tempdir().unwrap();
    let arm =
        Armazenamento::abrir(ConfigArmazenamento::arquivo(dir.path().join("cardeal.db"))).unwrap();
    arm.migrar(&[ModuloClientes.migracoes()]).unwrap();

    let empresa = Id::novo();
    let ctx = ContextoEscrita::novo(empresa, Id::novo(), Id::novo(), Id::novo());
    arm.escritor()
        .executar(ctx, move |uow| {
            uow.conexao()
                .execute(
                    "INSERT INTO nucleo_empresa
                       (id, razao_social, nome_fantasia, cnpj, regime, endereco, perfil, criado_em)
                     VALUES (?1,'Teste LTDA','Teste','11222333000181','SimplesNacional','{}','comercio',0)",
                    [empresa.em_bytes().as_slice()],
                )
                .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))
        })
        .unwrap();

    (dir, arm, empresa)
}

fn ambiente(empresa: Id) -> Ambiente {
    let mut rm = RegistroModulos::novo();
    rm.registrar(&MANIFESTO).unwrap();
    let efetivo = rm
        .resolver(&PedidoAtivacao::nova().com_modulo("clientes"))
        .unwrap();
    Ambiente::novo(empresa, efetivo)
}

fn sessao(empresa: Id, permissoes: &[&str]) -> Sessao {
    let mut papel = PapelAuth::novo(empresa, "Testador");
    for p in permissoes {
        papel = papel.com_permissao(*p);
    }
    Sessao::abrir(EmissaoSessao::padrao(
        Id::novo(),
        Id::novo(),
        Escopo::empresa_inteira(empresa),
        AutorizacoesEfetivas::consolidar([&papel]),
        Instante::EPOCA,
    ))
}

fn carga(v: &impl serde::Serialize) -> Vec<u8> {
    postcard::to_stdvec(v).unwrap()
}

fn conta_pessoas(arm: &Armazenamento, empresa: Id) -> i64 {
    arm.leitor()
        .consultar(|c| {
            c.query_row(
                "SELECT COUNT(*) FROM clientes_pessoa WHERE empresa = ?1",
                [empresa.em_bytes().as_slice()],
                |r| r.get(0),
            )
            .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))
        })
        .unwrap()
}

fn criar_pessoa(d: &Despachante, arm: &Armazenamento, empresa: Id, s: &Sessao) -> Id {
    let cmd = CriarPessoa {
        tipo: TipoPessoa::Juridica,
        nome: "Mercado Bom Preço LTDA".to_string(),
        nome_fantasia: Some("Mercado Bom Preço".to_string()),
        papel_inicial: Papel::Cliente,
        documento_tipo: TipoDocumento::Cnpj,
        documento_numero: "11.222.333/0001-81".to_string(),
    };
    let saida = d
        .executar_comando(
            "clientes.criar_pessoa.v1",
            &carga(&cmd),
            s,
            &ambiente(empresa),
            arm.escritor(),
        )
        .unwrap();
    postcard::from_bytes::<PessoaCadastrada>(&saida)
        .unwrap()
        .pessoa
}

#[test]
fn criar_pessoa_grava_o_cadastro_e_o_documento() {
    let (_dir, arm, empresa) = base();
    let d = Despachante::construir(&[&ModuloClientes]).unwrap();
    let s = sessao(empresa, &["clientes.pessoa.criar"]);

    let pessoa = criar_pessoa(&d, &arm, empresa, &s);
    assert_ne!(pessoa, Id::NULO);
    assert_eq!(conta_pessoas(&arm, empresa), 1);
}

#[test]
fn criar_pessoa_com_documento_repetido_e_recusado() {
    let (_dir, arm, empresa) = base();
    let d = Despachante::construir(&[&ModuloClientes]).unwrap();
    let s = sessao(empresa, &["clientes.pessoa.criar"]);
    criar_pessoa(&d, &arm, empresa, &s);

    let cmd = CriarPessoa {
        tipo: TipoPessoa::Juridica,
        nome: "Outra Razão Social LTDA".to_string(),
        nome_fantasia: None,
        papel_inicial: Papel::Fornecedor,
        documento_tipo: TipoDocumento::Cnpj,
        documento_numero: "11222333000181".to_string(),
    };
    let erro = d
        .executar_comando(
            "clientes.criar_pessoa.v1",
            &carga(&cmd),
            &s,
            &ambiente(empresa),
            arm.escritor(),
        )
        .unwrap_err();
    assert_eq!(erro.codigo, CodigoErro::DUPLICADO);
    assert_eq!(conta_pessoas(&arm, empresa), 1);
}

#[test]
fn sem_permissao_criar_pessoa_e_recusado() {
    let (_dir, arm, empresa) = base();
    let d = Despachante::construir(&[&ModuloClientes]).unwrap();
    let s = sessao(empresa, &["clientes.pessoa.ver"]);

    let cmd = CriarPessoa {
        tipo: TipoPessoa::Fisica,
        nome: "João Silva".to_string(),
        nome_fantasia: None,
        papel_inicial: Papel::Cliente,
        documento_tipo: TipoDocumento::Cpf,
        documento_numero: "52998224725".to_string(),
    };
    let erro = d
        .executar_comando(
            "clientes.criar_pessoa.v1",
            &carga(&cmd),
            &s,
            &ambiente(empresa),
            arm.escritor(),
        )
        .unwrap_err();
    assert_eq!(erro.codigo, CodigoErro::SEM_PERMISSAO);
    assert_eq!(conta_pessoas(&arm, empresa), 0);
}

#[test]
fn editar_pessoa_atualiza_o_nome() {
    let (_dir, arm, empresa) = base();
    let d = Despachante::construir(&[&ModuloClientes]).unwrap();
    let s = sessao(
        empresa,
        &["clientes.pessoa.criar", "clientes.pessoa.editar"],
    );
    let pessoa = criar_pessoa(&d, &arm, empresa, &s);

    let cmd = EditarPessoa {
        pessoa,
        nome: Some("Mercado Bom Preço Comércio LTDA".to_string()),
        nome_fantasia: None,
        observacao: Some("cliente desde 2020".to_string()),
    };
    let saida = d
        .executar_comando(
            "clientes.editar_pessoa.v1",
            &carga(&cmd),
            &s,
            &ambiente(empresa),
            arm.escritor(),
        )
        .unwrap();
    let editada: PessoaEditada = postcard::from_bytes(&saida).unwrap();
    assert_eq!(editada.pessoa, pessoa);
}

#[test]
fn adicionar_papel_coexiste_e_recusa_duplicata() {
    let (_dir, arm, empresa) = base();
    let d = Despachante::construir(&[&ModuloClientes]).unwrap();
    let s = sessao(
        empresa,
        &["clientes.pessoa.criar", "clientes.papel.gerenciar"],
    );
    let pessoa = criar_pessoa(&d, &arm, empresa, &s);

    let cmd = AdicionarPapel {
        pessoa,
        papel: Papel::Fornecedor,
    };
    let saida = d
        .executar_comando(
            "clientes.adicionar_papel.v1",
            &carga(&cmd),
            &s,
            &ambiente(empresa),
            arm.escritor(),
        )
        .unwrap();
    let resultado: PapelFoiAdicionado = postcard::from_bytes(&saida).unwrap();
    assert_eq!(resultado.papel, Papel::Fornecedor);

    let erro = d
        .executar_comando(
            "clientes.adicionar_papel.v1",
            &carga(&cmd),
            &s,
            &ambiente(empresa),
            arm.escritor(),
        )
        .unwrap_err();
    assert_eq!(erro.codigo, CodigoErro::DUPLICADO);
}

#[test]
fn definir_limite_credito_cria_e_depois_ajusta() {
    let (_dir, arm, empresa) = base();
    let d = Despachante::construir(&[&ModuloClientes]).unwrap();
    let s = sessao(
        empresa,
        &["clientes.pessoa.criar", "clientes.credito.definir_limite"],
    );
    let pessoa = criar_pessoa(&d, &arm, empresa, &s);

    let cmd = DefinirLimiteCredito {
        pessoa,
        limite: Dinheiro::reais(5000),
    };
    let saida = d
        .executar_comando(
            "clientes.definir_limite_credito.v1",
            &carga(&cmd),
            &s,
            &ambiente(empresa),
            arm.escritor(),
        )
        .unwrap();
    let definido: LimiteCreditoDefinido = postcard::from_bytes(&saida).unwrap();
    assert_eq!(definido.limite, Dinheiro::reais(5000));

    let cmd2 = DefinirLimiteCredito {
        pessoa,
        limite: Dinheiro::reais(8000),
    };
    let saida = d
        .executar_comando(
            "clientes.definir_limite_credito.v1",
            &carga(&cmd2),
            &s,
            &ambiente(empresa),
            arm.escritor(),
        )
        .unwrap();
    let definido: LimiteCreditoDefinido = postcard::from_bytes(&saida).unwrap();
    assert_eq!(definido.limite, Dinheiro::reais(8000));
}

#[test]
fn pessoas_por_papel_lista_e_filtra_por_busca() {
    let (_dir, arm, empresa) = base();
    let d = Despachante::construir(&[&ModuloClientes]).unwrap();
    let s = sessao(empresa, &["clientes.pessoa.criar", "clientes.pessoa.ver"]);
    let pessoa = criar_pessoa(&d, &arm, empresa, &s);

    let saida = d
        .executar_consulta(
            "clientes.pessoas_por_papel.v1",
            &carga(&PessoasPorPapel {
                papel: Papel::Cliente,
                busca: None,
            }),
            &s,
            &ambiente(empresa),
            arm.leitor(),
        )
        .unwrap();
    let itens: Vec<ItemPessoa> = postcard::from_bytes(&saida).unwrap();
    assert_eq!(itens.len(), 1);
    assert_eq!(itens[0].pessoa, pessoa);
    assert_eq!(itens[0].documento.as_deref(), Some("11222333000181"));

    let saida = d
        .executar_consulta(
            "clientes.pessoas_por_papel.v1",
            &carga(&PessoasPorPapel {
                papel: Papel::Cliente,
                busca: Some("não existe ninguém assim".to_string()),
            }),
            &s,
            &ambiente(empresa),
            arm.leitor(),
        )
        .unwrap();
    let vazio: Vec<ItemPessoa> = postcard::from_bytes(&saida).unwrap();
    assert!(vazio.is_empty());

    let saida = d
        .executar_consulta(
            "clientes.pessoas_por_papel.v1",
            &carga(&PessoasPorPapel {
                papel: Papel::Fornecedor,
                busca: None,
            }),
            &s,
            &ambiente(empresa),
            arm.leitor(),
        )
        .unwrap();
    let sem_papel: Vec<ItemPessoa> = postcard::from_bytes(&saida).unwrap();
    assert!(sem_papel.is_empty());
}

#[test]
fn detalhe_pessoa_traz_documentos_e_none_para_inexistente() {
    let (_dir, arm, empresa) = base();
    let d = Despachante::construir(&[&ModuloClientes]).unwrap();
    let s = sessao(empresa, &["clientes.pessoa.criar", "clientes.pessoa.ver"]);
    let pessoa = criar_pessoa(&d, &arm, empresa, &s);

    let saida = d
        .executar_consulta(
            "clientes.detalhe_pessoa.v1",
            &carga(&DetalhePessoa { pessoa }),
            &s,
            &ambiente(empresa),
            arm.leitor(),
        )
        .unwrap();
    let detalhe: Option<PessoaDetalhada> = postcard::from_bytes(&saida).unwrap();
    let detalhe = detalhe.unwrap();
    assert_eq!(detalhe.pessoa.id, pessoa);
    assert_eq!(detalhe.documentos.len(), 1);
    assert!(detalhe.limite_credito.is_none());

    let saida = d
        .executar_consulta(
            "clientes.detalhe_pessoa.v1",
            &carga(&DetalhePessoa { pessoa: Id::novo() }),
            &s,
            &ambiente(empresa),
            arm.leitor(),
        )
        .unwrap();
    let nenhuma: Option<PessoaDetalhada> = postcard::from_bytes(&saida).unwrap();
    assert!(nenhuma.is_none());
}

#[test]
fn adicionar_contato_e_endereco_aparecem_na_ficha() {
    let (_dir, arm, empresa) = base();
    let d = Despachante::construir(&[&ModuloClientes]).unwrap();
    let s = sessao(
        empresa,
        &[
            "clientes.pessoa.criar",
            "clientes.pessoa.editar",
            "clientes.pessoa.ver",
        ],
    );
    let pessoa = criar_pessoa(&d, &arm, empresa, &s);

    let celular: ContatoFoiAdicionado = postcard::from_bytes(
        &d.executar_comando(
            "clientes.adicionar_contato.v1",
            &carga(&AdicionarContato {
                pessoa,
                tipo: TipoContato::Celular,
                valor: "(31) 98888-7777".to_string(),
                principal: true,
            }),
            &s,
            &ambiente(empresa),
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_ne!(celular.contato, Id::NULO);

    let endereco: EnderecoFoiAdicionado = postcard::from_bytes(
        &d.executar_comando(
            "clientes.adicionar_endereco.v1",
            &carga(&AdicionarEndereco {
                pessoa,
                tipo: TipoEndereco::Comercial,
                logradouro: "Rua das Flores".to_string(),
                numero: "123".to_string(),
                complemento: None,
                bairro: "Centro".to_string(),
                cidade: "Belo Horizonte".to_string(),
                uf: "MG".to_string(),
                cep: "30110-010".to_string(),
                principal: true,
            }),
            &s,
            &ambiente(empresa),
            arm.escritor(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_ne!(endereco.endereco, Id::NULO);

    let saida = d
        .executar_consulta(
            "clientes.detalhe_pessoa.v1",
            &carga(&DetalhePessoa { pessoa }),
            &s,
            &ambiente(empresa),
            arm.leitor(),
        )
        .unwrap();
    let detalhe: PessoaDetalhada = postcard::from_bytes::<Option<PessoaDetalhada>>(&saida)
        .unwrap()
        .unwrap();
    assert_eq!(detalhe.contatos.len(), 1);
    assert_eq!(detalhe.contatos[0].valor, "31988887777");
    assert!(detalhe.contatos[0].principal);
    assert_eq!(detalhe.enderecos.len(), 1);
    assert_eq!(detalhe.enderecos[0].cidade, "Belo Horizonte");

    // Formato inválido é recusado.
    let erro = d
        .executar_comando(
            "clientes.adicionar_contato.v1",
            &carga(&AdicionarContato {
                pessoa,
                tipo: TipoContato::Email,
                valor: "nao-e-email".to_string(),
                principal: false,
            }),
            &s,
            &ambiente(empresa),
            arm.escritor(),
        )
        .unwrap_err();
    assert_eq!(erro.codigo, CodigoErro::ENTRADA_INVALIDA);
}
