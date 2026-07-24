// Caminho relativo: crates/cli/src/audit_sink.rs
//! Sink de auditoria persistente (MT-125, ADR-0037) — `FileAuditSink`
//! grava cada [`AuditEntry`]/[`GuardrailAuditEntry`] em `.agentry/audit.log`
//! (JSON Lines, uma entrada por linha), complementando (nunca substituindo)
//! o `StderrAuditSink` já existente em `main.rs`. `SinksCombinados` é o
//! combinador genérico que chama `record()` em dois sinks em sequência —
//! **não** reaproveita `ColetorDuplo` (`crates/core/src/session/mod.rs`,
//! privado ao módulo e com um propósito diferente: coletar
//! `GuardrailAuditEntry` para anexar a `SessionOutcome`, não combinar sinks
//! em geral).

use std::io::Write;
use std::path::PathBuf;

use agentry_core::egress::audit::AuditEntry;
use agentry_core::guardrail::GuardrailAuditEntry;
use agentry_core::guardrail::GuardrailAuditSink;
use agentry_core::transport::AuditSink;

/// Escreve cada entrada de auditoria em `.agentry/audit.log`, uma linha JSON
/// por entrada — nunca mantém um *file handle* aberto entre chamadas
/// (`OpenOptions::append` a cada `record()`, ADR-0037): mais simples, e
/// correto mesmo com duas invocações do `agentry` rodando em paralelo no
/// mesmo projeto. Falha ao gravar (diretório sem permissão, disco cheio) é
/// reportada em `stderr`, mas **nunca** interrompe a operação em
/// andamento — auditoria persistente é *best-effort*, não um novo ponto de
/// falha (ADR-0037, diretriz de conformidade).
pub struct FileAuditSink {
    workspace_root: PathBuf,
}

impl FileAuditSink {
    #[must_use]
    pub fn new(workspace_root: impl Into<PathBuf>) -> Self {
        Self {
            workspace_root: workspace_root.into(),
        }
    }

    fn escrever_linha(&self, linha: &str) {
        let resultado =
            agentry_core::state_dir::ensure_state_dir(&self.workspace_root).and_then(|estado| {
                std::fs::OpenOptions::new()
                    .append(true)
                    .create(true)
                    .open(estado.join("audit.log"))
                    .and_then(|mut arquivo| writeln!(arquivo, "{linha}"))
            });
        if let Err(erro) = resultado {
            eprintln!("[audit] falha ao gravar em .agentry/audit.log: {erro}");
        }
    }
}

impl AuditSink for FileAuditSink {
    fn record(&self, entry: AuditEntry) {
        match serde_json::to_string(&entry) {
            Ok(linha) => self.escrever_linha(&linha),
            Err(erro) => eprintln!("[audit] falha ao serializar AuditEntry: {erro}"),
        }
    }
}

impl GuardrailAuditSink for FileAuditSink {
    fn record(&self, entry: GuardrailAuditEntry) {
        match serde_json::to_string(&entry) {
            Ok(linha) => self.escrever_linha(&linha),
            Err(erro) => eprintln!("[audit] falha ao serializar GuardrailAuditEntry: {erro}"),
        }
    }
}

/// Combina dois sinks — chama `record()` em ambos, em sequência, para toda
/// entrada. Genérico sobre os dois tipos (não amarrado a `StderrAuditSink`/
/// `FileAuditSink` especificamente), mas só implementa [`AuditSink`]
/// quando ambos implementam [`AuditSink`] (idem para
/// [`GuardrailAuditSink`]) — o mesmo par `(A, B)` costuma implementar as
/// duas traits (`StderrAuditSink`/`FileAuditSink` implementam ambas), então
/// uma única `SinksCombinados` cobre os dois usos em `main.rs`.
pub struct SinksCombinados<A, B> {
    primeiro: A,
    segundo: B,
}

impl<A, B> SinksCombinados<A, B> {
    #[must_use]
    pub fn new(primeiro: A, segundo: B) -> Self {
        Self { primeiro, segundo }
    }
}

impl<A: AuditSink, B: AuditSink> AuditSink for SinksCombinados<A, B> {
    fn record(&self, entry: AuditEntry) {
        self.primeiro.record(entry.clone());
        self.segundo.record(entry);
    }
}

impl<A: GuardrailAuditSink, B: GuardrailAuditSink> GuardrailAuditSink for SinksCombinados<A, B> {
    fn record(&self, entry: GuardrailAuditEntry) {
        self.primeiro.record(entry.clone());
        self.segundo.record(entry);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    struct ColetorDeAudit {
        entradas: std::sync::Mutex<Vec<AuditEntry>>,
    }

    impl ColetorDeAudit {
        fn new() -> Self {
            Self {
                entradas: std::sync::Mutex::new(Vec::new()),
            }
        }
    }

    impl AuditSink for ColetorDeAudit {
        fn record(&self, entry: AuditEntry) {
            self.entradas
                .lock()
                .expect("lock não deve envenenar")
                .push(entry);
        }
    }

    struct ColetorDeGuardrail {
        entradas: std::sync::Mutex<Vec<GuardrailAuditEntry>>,
    }

    impl ColetorDeGuardrail {
        fn new() -> Self {
            Self {
                entradas: std::sync::Mutex::new(Vec::new()),
            }
        }
    }

    impl GuardrailAuditSink for ColetorDeGuardrail {
        fn record(&self, entry: GuardrailAuditEntry) {
            self.entradas
                .lock()
                .expect("lock não deve envenenar")
                .push(entry);
        }
    }

    fn entrada_de_teste() -> AuditEntry {
        AuditEntry::new(
            "http://exemplo.local",
            None,
            agentry_core::config::privacy::EgressClass::LocalOnly,
            "tarefa de teste",
            agentry_core::egress::audit::AuditOutcome::Allowed,
            None,
        )
    }

    fn entrada_guardrail_de_teste() -> GuardrailAuditEntry {
        GuardrailAuditEntry {
            direction: agentry_core::guardrail::GuardrailDirection::Input,
            rule_id: "regra-1".to_string(),
            action: agentry_core::guardrail::GuardrailAction::Redact,
            task: "tarefa de teste".to_string(),
        }
    }

    struct TempDir(PathBuf);
    impl TempDir {
        fn new() -> Self {
            let caminho = std::env::temp_dir().join(format!(
                "agentry-audit-sink-teste-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("relógio não deve estar antes de 1970")
                    .as_nanos()
            ));
            std::fs::create_dir_all(&caminho).expect("deve criar o diretório temporário");
            Self(caminho)
        }
        fn path(&self) -> &Path {
            &self.0
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn file_audit_sink_grava_uma_linha_json_por_entrada() {
        let dir = TempDir::new();
        let sink = FileAuditSink::new(dir.path());

        AuditSink::record(&sink, entrada_de_teste());
        AuditSink::record(&sink, entrada_de_teste());

        let conteudo =
            std::fs::read_to_string(dir.path().join(".agentry").join("audit.log")).unwrap();
        let linhas: Vec<&str> = conteudo.lines().collect();
        assert_eq!(linhas.len(), 2);
        for linha in linhas {
            let valor: serde_json::Value =
                serde_json::from_str(linha).expect("deve ser JSON válido");
            assert_eq!(valor["destination"], "http://exemplo.local");
            assert_eq!(valor["outcome"], "allowed");
        }
    }

    #[test]
    fn file_audit_sink_grava_entrada_de_guardrail() {
        let dir = TempDir::new();
        let sink = FileAuditSink::new(dir.path());

        GuardrailAuditSink::record(&sink, entrada_guardrail_de_teste());

        let conteudo =
            std::fs::read_to_string(dir.path().join(".agentry").join("audit.log")).unwrap();
        let valor: serde_json::Value = serde_json::from_str(conteudo.trim()).unwrap();
        assert_eq!(valor["rule_id"], "regra-1");
        assert_eq!(valor["direction"], "input");
    }

    #[test]
    fn sinks_combinados_chama_record_nos_dois_sinks_para_audit_entry() {
        let coletor_a = ColetorDeAudit::new();
        let coletor_b = ColetorDeAudit::new();
        let combinado = SinksCombinados::new(coletor_a, coletor_b);

        AuditSink::record(&combinado, entrada_de_teste());

        assert_eq!(combinado.primeiro.entradas.lock().unwrap().len(), 1);
        assert_eq!(combinado.segundo.entradas.lock().unwrap().len(), 1);
    }

    #[test]
    fn sinks_combinados_chama_record_nos_dois_sinks_para_guardrail_entry() {
        let coletor_a = ColetorDeGuardrail::new();
        let coletor_b = ColetorDeGuardrail::new();
        let combinado = SinksCombinados::new(coletor_a, coletor_b);

        GuardrailAuditSink::record(&combinado, entrada_guardrail_de_teste());

        assert_eq!(combinado.primeiro.entradas.lock().unwrap().len(), 1);
        assert_eq!(combinado.segundo.entradas.lock().unwrap().len(), 1);
    }

    #[test]
    fn file_audit_sink_combinado_com_stderr_grava_no_arquivo_mesmo_assim() {
        // Prova a composição real usada em `main.rs`: `SinksCombinados<Stderr,
        // File>` -- aqui um coletor no lugar do `StderrAuditSink` (que
        // escreve em stderr de verdade, ruidoso demais pra um teste), mas o
        // `FileAuditSink` é o de verdade, gravando em disco de fato.
        let dir = TempDir::new();
        let combinado = SinksCombinados::new(ColetorDeAudit::new(), FileAuditSink::new(dir.path()));

        AuditSink::record(&combinado, entrada_de_teste());

        assert_eq!(combinado.primeiro.entradas.lock().unwrap().len(), 1);
        let conteudo =
            std::fs::read_to_string(dir.path().join(".agentry").join("audit.log")).unwrap();
        assert!(!conteudo.trim().is_empty());
    }
}
