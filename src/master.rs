// src/main.rs — Etape 5 : Journalisation fichier
use std::fmt;
use chrono::Local;
use sysinfo::{System, Process};
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use std::fs::OpenOptions;

// --- Structures de données ---

#[derive(Debug, Clone)]
struct CpuInfo {
    usage_percent: f32,
    core_count: usize,
}

#[derive(Debug, Clone)]
struct MemInfo {
    total_mb: u64,
    used_mb: u64,
    free_mb: u64,
}

#[derive(Debug, Clone)]
struct ProcessInfo {
    pid: u32,
    name: String,
    cpu_usage: f32,
    memory_mb: u64,
}

#[derive(Debug, Clone)]
struct SystemSnapshot {
    timestamp: String,
    cpu: CpuInfo,
    memory: MemInfo,
    top_processes: Vec<ProcessInfo>,
}

// --- Trait Display : affichage humain ---

impl fmt::Display for CpuInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "CPU: {:.1}% ({} cœurs)", self.usage_percent, self.core_count)
    }
}

impl fmt::Display for MemInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "MEM: {}MB utilisés / {}MB total ({} MB libres)",
            self.used_mb, self.total_mb, self.free_mb
        )
    }
}

impl fmt::Display for ProcessInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "  [{:>6}] {:<25} CPU:{:>5.1}%  MEM:{:>5}MB",
            self.pid, self.name, self.cpu_usage, self.memory_mb
        )
    }
}

impl fmt::Display for SystemSnapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "=== SysWatch — {} ===", self.timestamp)?;
        writeln!(f, "{}", self.cpu)?;
        writeln!(f, "{}", self.memory)?;
        writeln!(f, "--- Top Processus ---")?;
        for p in &self.top_processes {
            writeln!(f, "{}", p)?;
        }
        write!(f, "=====================")
    }
}

// --- Etape 2 : Enum d'erreur personnalisée ---

#[derive(Debug)]
enum SysWatchError {
    CollectionFailed(String),
}

impl fmt::Display for SysWatchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SysWatchError::CollectionFailed(msg) => write!(f, "Erreur collecte: {}", msg),
        }
    }
}

impl std::error::Error for SysWatchError {}

// --- Etape 2 : Collecte des vraies métriques système ---

fn collect_snapshot() -> Result<SystemSnapshot, SysWatchError> {
    let mut sys = System::new_all();
    sys.refresh_all();

    // Pause pour laisser sysinfo mesurer l'activité CPU (sinon on obtient 0%)
    std::thread::sleep(std::time::Duration::from_millis(500));
    sys.refresh_all();

    let cpu_usage = sys.global_cpu_info().cpu_usage();
    let core_count = sys.cpus().len();

    if core_count == 0 {
        return Err(SysWatchError::CollectionFailed("Aucun CPU détecté".to_string()));
    }

    let total_mb = sys.total_memory() / 1024 / 1024;
    let used_mb  = sys.used_memory()  / 1024 / 1024;
    let free_mb  = sys.free_memory()  / 1024 / 1024;

    // Collecter tous les processus, trier par CPU, garder le top 5
    let mut processes: Vec<ProcessInfo> = sys
        .processes()
        .values()
        .map(|p: &Process| ProcessInfo {
            pid:        p.pid().as_u32(),
            name:       p.name().to_string(),
            cpu_usage:  p.cpu_usage(),
            memory_mb:  p.memory() / 1024 / 1024,
        })
        .collect();

    processes.sort_by(|a, b| b.cpu_usage.partial_cmp(&a.cpu_usage).unwrap());
    processes.truncate(5);

    Ok(SystemSnapshot {
        timestamp:      Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        cpu:            CpuInfo { usage_percent: cpu_usage, core_count },
        memory:         MemInfo { total_mb, used_mb, free_mb },
        top_processes:  processes,
    })
}

// --- Etape 3 : Formatage des réponses selon la commande reçue ---

fn format_response(snapshot: &SystemSnapshot, command: &str) -> String {
    let cmd = command.trim().to_lowercase();

    match cmd.as_str() {

        "cpu" => {
            // Barre ASCII : 10 blocs, chaque bloc = 10% de CPU
            let filled = (snapshot.cpu.usage_percent / 10.0) as usize;
            let bar: String = (0..10)
                .map(|i| if i < filled { "█" } else { "░" })
                .collect::<Vec<_>>()
                .join("");
            format!(
                "[CPU]\n{}\n[{}] {:.1}%\n",
                snapshot.cpu, bar, snapshot.cpu.usage_percent
            )
        }

        "mem" => {
            // Barre ASCII : 20 blocs, chaque bloc = 5% de RAM
            let percent = snapshot.memory.used_mb as f64
                / snapshot.memory.total_mb as f64
                * 100.0;
            let bar: String = (0..20)
                .map(|i| if i < (percent / 5.0) as usize { '█' } else { '░' })
                .collect();
            format!(
                "[MÉMOIRE]\n{}\n[{}] {:.1}%\n",
                snapshot.memory, bar, percent
            )
        }

        "ps" | "procs" => {
            // Itérateur avec enumerate() pour numéroter les lignes
            let lines: String = snapshot
                .top_processes
                .iter()
                .enumerate()
                .map(|(i, p)| format!("{}. {}", i + 1, p))
                .collect::<Vec<_>>()
                .join("\n");
            format!(
                "[PROCESSUS — Top {}]\n{}\n",
                snapshot.top_processes.len(),
                lines
            )
        }

        "all" | "" => format!("{}\n", snapshot),

        "help" => concat!(
            "Commandes disponibles:\n",
            "  cpu   — Usage CPU + barre\n",
            "  mem   — Mémoire RAM\n",
            "  ps    — Top processus\n",
            "  all   — Vue complète\n",
            "  help  — Cette aide\n",
            "  quit  — Fermer la connexion\n",
        ).to_string(),

        "quit" | "exit" => "BYE\n".to_string(),

        _ => format!("Commande inconnue: '{}'. Tape 'help'.\n", command.trim()),
    }
}

// --- Etape 5 : Journalisation horodatée dans syswatch.log ---

fn log_event(message: &str) {
    let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let line = format!("[{}] {}\n", timestamp, message);

    // Affichage console
    print!("{}", line);

    // Écriture en mode append (création si inexistant, jamais écrasé)
    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open("syswatch.log")
    {
        let _ = file.write_all(line.as_bytes());
    }
}

// --- Etape 4 : Thread de rafraîchissement du snapshot toutes les 5s ---

fn snapshot_refresher(snapshot: Arc<Mutex<SystemSnapshot>>) {
    loop {
        thread::sleep(Duration::from_secs(5));
        match collect_snapshot() {
            Ok(new_snap) => {
                let mut snap = snapshot.lock().unwrap();
                *snap = new_snap;
                println!("[refresh] Métriques mises à jour");
            }
            Err(e) => eprintln!("[refresh] Erreur: {}", e),
        }
    }
}

// --- Etape 4 : Gestion d'un client dans son propre thread ---

fn handle_client(mut stream: TcpStream, snapshot: Arc<Mutex<SystemSnapshot>>) {
    let peer = stream.peer_addr()
        .map(|a| a.to_string())
        .unwrap_or("inconnu".to_string());
    log_event(&format!("[+] Connexion de {}", peer));

    // Message de bienvenue
    let welcome = concat!(
        "╔══════════════════════════════╗\n",
        "║   SysWatch v1.0              ║\n",
        "║   Tape 'help' pour commencer ║\n",
        "╚══════════════════════════════╝\n",
        "> "
    );
    let _ = stream.write_all(welcome.as_bytes());

    // BufReader pour lire ligne par ligne sans bloquer les autres threads
    let reader = BufReader::new(stream.try_clone().expect("Clonage stream échoué"));

    for line in reader.lines() {
        match line {
            Ok(cmd) => {
                let cmd = cmd.trim().to_string();
                log_event(&format!("[{}] commande: '{}'", peer, cmd));

                if cmd.eq_ignore_ascii_case("quit") || cmd.eq_ignore_ascii_case("exit") {
                    let _ = stream.write_all(b"Au revoir!\n");
                    break;
                }

                // Verrouiller le snapshot partagé le temps de lire, puis relâcher
                let response = {
                    let snap = snapshot.lock().unwrap();
                    format_response(&snap, &cmd)
                };

                let _ = stream.write_all(response.as_bytes());
                let _ = stream.write_all(b"> "); // prompt
            }
            Err(_) => break,
        }
    }

    log_event(&format!("[-] Déconnexion de {}", peer));
}

// --- Main Etape 4 : lancement du serveur ---

fn main() {
    log_event("=== SysWatch démarrage ===");

    // Collecte initiale (bloquante)
    let initial = collect_snapshot().expect("Impossible de collecter les métriques initiales");
    log_event("Métriques initiales collectées");

    // Arc<Mutex<T>> : pointeur partagé + verrou entre threads
    let shared_snapshot = Arc::new(Mutex::new(initial));

    // Thread de rafraîchissement automatique toutes les 5s
    {
        let snap_clone = Arc::clone(&shared_snapshot);
        thread::spawn(move || snapshot_refresher(snap_clone));
    }

    // Démarrage du serveur TCP sur toutes les interfaces
    let listener = TcpListener::bind("0.0.0.0:7878")
        .expect("Impossible de bind le port 7878");

    log_event("Serveur en écoute sur le port 7878");
    println!("Connecte-toi avec : telnet localhost 7878");
    println!("              ou  : nc localhost 7878  (WSL/Git Bash)");
    println!("Ctrl+C pour arrêter.\n");

    // Boucle principale : accept() bloque jusqu'à une nouvelle connexion
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let snap_clone = Arc::clone(&shared_snapshot);
                // Chaque client dans son propre thread
                thread::spawn(move || handle_client(stream, snap_clone));
            }
            Err(e) => eprintln!("Erreur connexion entrante: {}", e),
        }
    }
}