//! O `impl Modulo` do cadastro de pessoas — o ponto por onde o motor coleta manifesto,
//! migrações e comandos (`docs/contratos-internos.md` §4). Sem lógica: só amarração.

use cardeal_kernel::Resultado;
use cardeal_modkit::{Manifesto, Modulo, Registro};
use cardeal_storage::ConjuntoMigracoes;

use crate::comandos::{
    AdicionarContato, AdicionarEndereco, AdicionarPapel, CriarPessoa, DefinirLimiteCredito,
    EditarPessoa,
};
use crate::consultas::{DetalhePessoa, PessoasPorPapel};
use crate::manifesto::MANIFESTO;
use crate::migracoes;

/// O módulo de clientes, para registrar no [`Despachante`](cardeal_modkit::Despachante).
pub struct ModuloClientes;

impl Modulo for ModuloClientes {
    fn manifesto(&self) -> &'static Manifesto {
        &MANIFESTO
    }

    fn migracoes(&self) -> ConjuntoMigracoes {
        migracoes::conjunto()
    }

    fn registrar(&self, registro: &mut Registro) -> Resultado<()> {
        registro
            .comando::<CriarPessoa>("clientes.criar_pessoa.v1")
            .comando::<EditarPessoa>("clientes.editar_pessoa.v1")
            .comando::<AdicionarPapel>("clientes.adicionar_papel.v1")
            .comando::<DefinirLimiteCredito>("clientes.definir_limite_credito.v1")
            .comando::<AdicionarContato>("clientes.adicionar_contato.v1")
            .comando::<AdicionarEndereco>("clientes.adicionar_endereco.v1")
            .consulta::<DetalhePessoa>("clientes.detalhe_pessoa.v1")
            .consulta::<PessoasPorPapel>("clientes.pessoas_por_papel.v1");
        Ok(())
    }
}
