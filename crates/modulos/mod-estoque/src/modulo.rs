//! O `impl Modulo` do estoque — o ponto por onde o motor coleta manifesto, migrações e
//! comandos (`docs/contratos-internos.md` §4). Sem lógica: só amarração.

use cardeal_kernel::Resultado;
use cardeal_modkit::{Manifesto, Modulo, Registro};
use cardeal_storage::ConjuntoMigracoes;

use crate::comandos::{
    AjustarSaldo, CriarGrupoProduto, CriarLocal, CriarProduto, CriarUnidade, RegistrarEntrada,
    RegistrarSaida,
};
use crate::consultas::{GruposProduto, Locais, ProdutoPorCodigoBarras, ProdutosComSaldo, Unidades};
use crate::manifesto::MANIFESTO;
use crate::migracoes;

/// O módulo de estoque, para registrar no [`Despachante`](cardeal_modkit::Despachante).
pub struct ModuloEstoque;

impl Modulo for ModuloEstoque {
    fn manifesto(&self) -> &'static Manifesto {
        &MANIFESTO
    }

    fn migracoes(&self) -> ConjuntoMigracoes {
        migracoes::conjunto()
    }

    fn registrar(&self, registro: &mut Registro) -> Resultado<()> {
        registro
            .comando::<CriarGrupoProduto>("estoque.criar_grupo_produto.v1")
            .comando::<CriarUnidade>("estoque.criar_unidade.v1")
            .comando::<CriarProduto>("estoque.criar_produto.v1")
            .comando::<CriarLocal>("estoque.criar_local.v1")
            .comando::<RegistrarEntrada>("estoque.registrar_entrada.v1")
            .comando::<RegistrarSaida>("estoque.registrar_saida.v1")
            .comando::<AjustarSaldo>("estoque.ajustar_saldo.v1")
            .consulta::<ProdutosComSaldo>("estoque.produtos_com_saldo.v1")
            .consulta::<ProdutoPorCodigoBarras>("estoque.produto_por_codigo_barras.v1")
            .consulta::<GruposProduto>("estoque.grupos_produto.v1")
            .consulta::<Unidades>("estoque.unidades.v1")
            .consulta::<Locais>("estoque.locais.v1");
        Ok(())
    }
}
