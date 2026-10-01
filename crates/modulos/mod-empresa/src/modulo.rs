//! O `impl Modulo` — só amarração. Sem migração própria: as tabelas são do núcleo.

use cardeal_kernel::Resultado;
use cardeal_modkit::{Manifesto, Modulo, Registro};
use cardeal_storage::ConjuntoMigracoes;

use crate::comandos::{AtualizarDadosEmpresa, DefinirContatoEmpresa, DefinirLogoEmpresa};
use crate::consultas::{DadosDaEmpresa, IdentidadeDaEmpresa, PapeisDaEmpresa, UsuariosDaEmpresa};
use crate::manifesto::MANIFESTO;

/// O módulo, para registrar no [`Despachante`](cardeal_modkit::Despachante).
pub struct ModuloEmpresa;

impl Modulo for ModuloEmpresa {
    fn manifesto(&self) -> &'static Manifesto {
        &MANIFESTO
    }

    fn migracoes(&self) -> ConjuntoMigracoes {
        ConjuntoMigracoes {
            modulo: "empresa",
            depende_de: &["nucleo"],
            migracoes: &[],
        }
    }

    fn registrar(&self, registro: &mut Registro) -> Resultado<()> {
        registro
            .comando::<AtualizarDadosEmpresa>("empresa.atualizar_dados.v1")
            .comando::<DefinirContatoEmpresa>("empresa.definir_contato.v1")
            .comando::<DefinirLogoEmpresa>("empresa.definir_logo.v1")
            .consulta::<DadosDaEmpresa>("empresa.dados.v1")
            .consulta::<IdentidadeDaEmpresa>("empresa.identidade_visual.v1")
            .consulta::<UsuariosDaEmpresa>("empresa.usuarios.v1")
            .consulta::<PapeisDaEmpresa>("empresa.papeis.v1");
        Ok(())
    }
}
