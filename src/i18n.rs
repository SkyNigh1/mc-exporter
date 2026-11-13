//! User interface translations. The key is the English text; each entry gives the
//! French, Spanish, German, Portuguese and Italian versions. Placeholders: {0}, {1}…

use std::collections::HashMap;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize, Default)]
pub enum Lang {
    #[default]
    En,
    Fr,
    Es,
    De,
    Pt,
    It,
}

impl Lang {
    pub const ALL: [Lang; 6] = [Lang::En, Lang::Fr, Lang::Es, Lang::De, Lang::Pt, Lang::It];

    /// Name of the language, written in that language
    pub fn native_name(self) -> &'static str {
        match self {
            Lang::En => "English",
            Lang::Fr => "Français",
            Lang::Es => "Español",
            Lang::De => "Deutsch",
            Lang::Pt => "Português",
            Lang::It => "Italiano",
        }
    }
}

static CURRENT: AtomicU8 = AtomicU8::new(0);

pub fn set(lang: Lang) {
    CURRENT.store(lang as u8, Ordering::Relaxed);
}

pub fn current() -> Lang {
    Lang::ALL[CURRENT.load(Ordering::Relaxed) as usize % Lang::ALL.len()]
}

/// (English, [French, Spanish, German, Portuguese, Italian])
const TABLE: &[(&str, [&str; 5])] = &[
    // ── general ──
    ("Language", ["Langue", "Idioma", "Sprache", "Idioma", "Lingua"]),
    ("World", ["Monde", "Mundo", "Welt", "Mundo", "Mondo"]),
    ("(other folder)", ["(autre dossier)", "(otra carpeta)", "(anderer Ordner)", "(outra pasta)", "(altra cartella)"]),
    ("Choose a world folder", ["Choisir un dossier de monde", "Elegir una carpeta de mundo", "Weltordner auswählen", "Escolher uma pasta de mundo", "Scegli una cartella del mondo"]),
    ("Import a schematic…", ["Importer un schematic…", "Importar un schematic…", "Schematic importieren…", "Importar um schematic…", "Importa uno schematic…"]),
    (".schem (Sponge v1/v2/v3), .schematic (legacy MCEdit), .litematic, .nbt — you can also drop the file onto the window", [
        ".schem (Sponge v1/v2/v3), .schematic (ancien MCEdit), .litematic, .nbt — tu peux aussi glisser le fichier sur la fenêtre",
        ".schem (Sponge v1/v2/v3), .schematic (MCEdit antiguo), .litematic, .nbt — también puedes soltar el archivo sobre la ventana",
        ".schem (Sponge v1/v2/v3), .schematic (altes MCEdit), .litematic, .nbt — du kannst die Datei auch auf das Fenster ziehen",
        ".schem (Sponge v1/v2/v3), .schematic (MCEdit antigo), .litematic, .nbt — também pode arrastar o arquivo para a janela",
        ".schem (Sponge v1/v2/v3), .schematic (vecchio MCEdit), .litematic, .nbt — puoi anche trascinare il file sulla finestra",
    ]),
    ("Load", ["Charger", "Cargar", "Laden", "Carregar", "Carica"]),
    ("Dimension", ["Dimension", "Dimensión", "Dimension", "Dimensão", "Dimensione"]),
    ("{0} (empty)", ["{0} (vide)", "{0} (vacía)", "{0} (leer)", "{0} (vazia)", "{0} (vuota)"]),
    ("Settings", ["Paramètres", "Ajustes", "Einstellungen", "Configurações", "Impostazioni"]),
    ("Go to…", ["Aller à…", "Ir a…", "Gehe zu…", "Ir para…", "Vai a…"]),
    ("Floor", ["Plancher", "Suelo", "Boden", "Piso", "Pavimento"]),
    ("Ceiling", ["Plafond", "Techo", "Decke", "Teto", "Soffitto"]),
    ("Refresh map", ["Actualiser la carte", "Actualizar el mapa", "Karte aktualisieren", "Atualizar o mapa", "Aggiorna la mappa"]),
    ("Show chunks", ["Afficher les chunks", "Mostrar los chunks", "Chunks anzeigen", "Mostrar os chunks", "Mostra i chunk"]),
    ("Position 1", ["Position 1", "Posición 1", "Position 1", "Posição 1", "Posizione 1"]),
    ("Position 2", ["Position 2", "Posición 2", "Position 2", "Posição 2", "Posizione 2"]),
    ("{0} × {1} × {2} blocks", ["{0} × {1} × {2} blocs", "{0} × {1} × {2} bloques", "{0} × {1} × {2} Blöcke", "{0} × {1} × {2} blocos", "{0} × {1} × {2} blocchi"]),
    ("Clear selection", ["Effacer la sélection", "Borrar la selección", "Auswahl löschen", "Limpar a seleção", "Cancella la selezione"]),
    // ── status ──
    ("No Minecraft version found in .minecraft/versions: launch the game once or pick a jar in the settings.", [
        "Aucune version de Minecraft trouvée dans .minecraft/versions : lance le jeu une fois ou choisis un jar dans les paramètres.",
        "No se encontró ninguna versión de Minecraft en .minecraft/versions: inicia el juego una vez o elige un jar en los ajustes.",
        "Keine Minecraft-Version in .minecraft/versions gefunden: starte das Spiel einmal oder wähle eine JAR in den Einstellungen.",
        "Nenhuma versão do Minecraft encontrada em .minecraft/versions: abra o jogo uma vez ou escolha um jar nas configurações.",
        "Nessuna versione di Minecraft trovata in .minecraft/versions: avvia il gioco una volta o scegli un jar nelle impostazioni.",
    ]),
    ("Loading world…", ["Chargement du monde…", "Cargando el mundo…", "Welt wird geladen…", "Carregando o mundo…", "Caricamento del mondo…"]),
    ("Importing {0}…", ["Import de {0}…", "Importando {0}…", "{0} wird importiert…", "Importando {0}…", "Importazione di {0}…"]),
    ("No Minecraft jar found (Settings → pick a jar)", [
        "Aucun jar Minecraft trouvé (Paramètres → choisir un jar)",
        "No se encontró ningún jar de Minecraft (Ajustes → elegir un jar)",
        "Keine Minecraft-JAR gefunden (Einstellungen → JAR wählen)",
        "Nenhum jar do Minecraft encontrado (Configurações → escolher um jar)",
        "Nessun jar di Minecraft trovato (Impostazioni → scegli un jar)",
    ]),
    ("Schematic “{0}” — {1} — {2} × {3} × {4} — {5} blocks — resources: {6}", [
        "Schematic « {0} » — {1} — {2} × {3} × {4} — {5} blocs — ressources : {6}",
        "Schematic «{0}» — {1} — {2} × {3} × {4} — {5} bloques — recursos: {6}",
        "Schematic „{0}“ — {1} — {2} × {3} × {4} — {5} Blöcke — Ressourcen: {6}",
        "Schematic “{0}” — {1} — {2} × {3} × {4} — {5} blocos — recursos: {6}",
        "Schematic «{0}» — {1} — {2} × {3} × {4} — {5} blocchi — risorse: {6}",
    ]),
    ("“{0}” — Minecraft {1} — resources: {2}", [
        "« {0} » — Minecraft {1} — ressources : {2}",
        "«{0}» — Minecraft {1} — recursos: {2}",
        "„{0}“ — Minecraft {1} — Ressourcen: {2}",
        "“{0}” — Minecraft {1} — recursos: {2}",
        "«{0}» — Minecraft {1} — risorse: {2}",
    ]),
    ("Error: {0}", ["Erreur : {0}", "Error: {0}", "Fehler: {0}", "Erro: {0}", "Errore: {0}"]),
    ("Load a world and select an area on the map (right-click + drag).", [
        "Charge un monde et sélectionne une zone sur la carte (clic droit + glisser).",
        "Carga un mundo y selecciona una zona en el mapa (clic derecho + arrastrar).",
        "Lade eine Welt und wähle einen Bereich auf der Karte (Rechtsklick + Ziehen).",
        "Carregue um mundo e selecione uma área no mapa (clique direito + arrastar).",
        "Carica un mondo e seleziona un'area sulla mappa (clic destro + trascina).",
    ]),
    ("Exported: {0} — {1} faces, {2} vertices, {3} materials, {4} MB", [
        "Exporté : {0} — {1} faces, {2} sommets, {3} matériaux, {4} Mo",
        "Exportado: {0} — {1} caras, {2} vértices, {3} materiales, {4} MB",
        "Exportiert: {0} — {1} Flächen, {2} Eckpunkte, {3} Materialien, {4} MB",
        "Exportado: {0} — {1} faces, {2} vértices, {3} materiais, {4} MB",
        "Esportato: {0} — {1} facce, {2} vertici, {3} materiali, {4} MB",
    ]),
    ("Unrecognized file: drop a .schem, .schematic, .litematic, .nbt or a world folder.", [
        "Fichier non reconnu : glisse un .schem, .schematic, .litematic, .nbt ou un dossier de monde.",
        "Archivo no reconocido: suelta un .schem, .schematic, .litematic, .nbt o una carpeta de mundo.",
        "Datei nicht erkannt: ziehe eine .schem-, .schematic-, .litematic-, .nbt-Datei oder einen Weltordner hierher.",
        "Arquivo não reconhecido: arraste um .schem, .schematic, .litematic, .nbt ou uma pasta de mundo.",
        "File non riconosciuto: trascina un .schem, .schematic, .litematic, .nbt o una cartella del mondo.",
    ]),
    // ── export panel ──
    ("OBJ export", ["Export OBJ", "Exportación OBJ", "OBJ-Export", "Exportação OBJ", "Esportazione OBJ"]),
    ("Scale (size of one block)", ["Échelle (taille d'un bloc)", "Escala (tamaño de un bloque)", "Maßstab (Größe eines Blocks)", "Escala (tamanho de um bloco)", "Scala (dimensione di un blocco)"]),
    ("Offset", ["Décalage", "Desplazamiento", "Versatz", "Deslocamento", "Spostamento"]),
    ("None", ["Aucun", "Ninguno", "Keiner", "Nenhum", "Nessuno"]),
    ("Center", ["Centrer", "Centrar", "Zentrieren", "Centralizar", "Centra"]),
    ("Custom", ["Personnalisé", "Personalizado", "Benutzerdefiniert", "Personalizado", "Personalizzato"]),
    ("Content", ["Contenu", "Contenido", "Inhalt", "Conteúdo", "Contenuto"]),
    ("Choose blocks to export…", ["Choisir les blocs à exporter…", "Elegir los bloques a exportar…", "Zu exportierende Blöcke wählen…", "Escolher os blocos a exportar…", "Scegli i blocchi da esportare…"]),
    ("Choose blocks to export… ({0} excluded)", [
        "Choisir les blocs à exporter… ({0} exclus)",
        "Elegir los bloques a exportar… ({0} excluidos)",
        "Zu exportierende Blöcke wählen… ({0} ausgeschlossen)",
        "Escolher os blocos a exportar… ({0} excluídos)",
        "Scegli i blocchi da esportare… ({0} esclusi)",
    ]),
    ("Show unknown blocks (mods)", ["Afficher les blocs inconnus (mods)", "Mostrar bloques desconocidos (mods)", "Unbekannte Blöcke anzeigen (Mods)", "Mostrar blocos desconhecidos (mods)", "Mostra blocchi sconosciuti (mod)"]),
    ("World sides and bottom", ["Côtés et dessous du monde", "Lados y fondo del mundo", "Weltseiten und -boden", "Laterais e fundo do mundo", "Lati e fondo del mondo"]),
    ("Biome colors", ["Couleurs des biomes", "Colores de bioma", "Biomfarben", "Cores dos biomas", "Colori dei biomi"]),
    ("Convert ores to stone", ["Convertir les minerais en pierre", "Convertir minerales en piedra", "Erze in Stein umwandeln", "Converter minérios em pedra", "Converti i minerali in pietra"]),
    ("Chests, banners, heads, signs…", ["Coffres, bannières, têtes, panneaux…", "Cofres, estandartes, cabezas, carteles…", "Truhen, Banner, Köpfe, Schilder…", "Baús, estandartes, cabeças, placas…", "Bauli, stendardi, teste, cartelli…"]),
    ("Sign text", ["Texte des panneaux", "Texto de los carteles", "Schildtext", "Texto das placas", "Testo dei cartelli"]),
    ("Entities (mobs, displays, item frames, paintings…)", [
        "Entités (mobs, displays, cadres, tableaux…)",
        "Entidades (criaturas, displays, marcos, cuadros…)",
        "Objekte (Kreaturen, Displays, Rahmen, Gemälde…)",
        "Entidades (criaturas, displays, molduras, pinturas…)",
        "Entità (creature, display, cornici, dipinti…)",
    ]),
    ("Download player head skins", ["Télécharger les skins des têtes de joueur", "Descargar skins de cabezas de jugador", "Skins von Spielerköpfen herunterladen", "Baixar skins das cabeças de jogador", "Scarica le skin delle teste dei giocatori"]),
    ("Skins already seen in game are read from the Minecraft cache; the others are requested from Mojang's servers.", [
        "Les skins déjà vues en jeu sont lues dans le cache de Minecraft ; les autres sont demandées aux serveurs de Mojang.",
        "Las skins ya vistas en el juego se leen de la caché de Minecraft; las demás se piden a los servidores de Mojang.",
        "Bereits im Spiel gesehene Skins werden aus dem Minecraft-Cache gelesen; die anderen werden von den Mojang-Servern geladen.",
        "As skins já vistas no jogo são lidas do cache do Minecraft; as outras são pedidas aos servidores da Mojang.",
        "Le skin già viste in gioco vengono lette dalla cache di Minecraft; le altre vengono richieste ai server di Mojang.",
    ]),
    ("Place ghasts on their tentacles", ["Poser les ghasts sur leurs tentacules", "Apoyar los ghasts sobre sus tentáculos", "Ghasts auf ihre Tentakel stellen", "Apoiar os ghasts nos tentáculos", "Appoggia i ghast sui tentacoli"]),
    ("In the game, ghast tentacles hang below their position. Check this to raise them to ground level.", [
        "Dans le jeu, les tentacules des ghasts pendent sous leur position. Cochez pour les remonter au niveau du sol.",
        "En el juego, los tentáculos de los ghasts cuelgan por debajo de su posición. Márcalo para subirlos al nivel del suelo.",
        "Im Spiel hängen die Tentakel der Ghasts unter ihrer Position. Aktivieren, um sie auf Bodenhöhe anzuheben.",
        "No jogo, os tentáculos dos ghasts ficam pendurados abaixo da sua posição. Marque para elevá-los ao nível do chão.",
        "Nel gioco, i tentacoli dei ghast pendono sotto la loro posizione. Seleziona per sollevarli al livello del suolo.",
    ]),
    ("Structure", ["Structure", "Estructura", "Struktur", "Estrutura", "Struttura"]),
    ("One object per material", ["Un objet par matériau", "Un objeto por material", "Ein Objekt pro Material", "Um objeto por material", "Un oggetto per materiale"]),
    ("One object per chunk", ["Un objet par chunk", "Un objeto por chunk", "Ein Objekt pro Chunk", "Um objeto por chunk", "Un oggetto per chunk"]),
    ("Optimize mesh (merge faces)", ["Optimiser le maillage (fusion des faces)", "Optimizar la malla (fusionar caras)", "Netz optimieren (Flächen zusammenführen)", "Otimizar a malha (fundir faces)", "Ottimizza la mesh (unisci le facce)"]),
    ("Double-sided faces (plants, thin panes)", ["Faces recto-verso (plantes, vitres fines)", "Caras de doble cara (plantas, paneles finos)", "Doppelseitige Flächen (Pflanzen, dünne Scheiben)", "Faces de dupla face (plantas, painéis finos)", "Facce a doppio lato (piante, pannelli sottili)"]),
    ("Random model variants", ["Variantes aléatoires des modèles", "Variantes aleatorias de modelos", "Zufällige Modellvarianten", "Variantes aleatórias de modelos", "Varianti casuali dei modelli"]),
    ("OBJ groups “g” (Maya compatible)", ["Groupes OBJ « g » (compatible Maya)", "Grupos OBJ «g» (compatible con Maya)", "OBJ-Gruppen „g“ (Maya-kompatibel)", "Grupos OBJ “g” (compatível com Maya)", "Gruppi OBJ «g» (compatibile con Maya)"]),
    ("Textures", ["Textures", "Texturas", "Texturen", "Texturas", "Texture"]),
    ("Export textures", ["Exporter les textures", "Exportar las texturas", "Texturen exportieren", "Exportar as texturas", "Esporta le texture"]),
    ("Upscale", ["Agrandir", "Ampliar", "Vergrößern", "Ampliar", "Ingrandisci"]),
    ("Alpha channel in separate files", ["Canal alpha dans des fichiers séparés", "Canal alfa en archivos separados", "Alphakanal in separaten Dateien", "Canal alfa em arquivos separados", "Canale alfa in file separati"]),
    ("Export…", ["Exporter…", "Exportar…", "Exportieren…", "Exportar…", "Esporta…"]),
    ("Select an area: right-click + drag on the map.", [
        "Sélectionne une zone : clic droit + glisser sur la carte.",
        "Selecciona una zona: clic derecho + arrastrar sobre el mapa.",
        "Wähle einen Bereich: Rechtsklick + Ziehen auf der Karte.",
        "Selecione uma área: clique direito + arrastar no mapa.",
        "Seleziona un'area: clic destro + trascina sulla mappa.",
    ]),
    ("Cancel", ["Annuler", "Cancelar", "Abbrechen", "Cancelar", "Annulla"]),
    ("Open folder", ["Ouvrir le dossier", "Abrir la carpeta", "Ordner öffnen", "Abrir a pasta", "Apri la cartella"]),
    // ── map ──
    ("{0}×   left click: pan · right click (or Shift+click): select · wheel: zoom", [
        "{0}×   clic gauche : déplacer · clic droit (ou Maj+clic) : sélectionner · molette : zoom",
        "{0}×   clic izquierdo: mover · clic derecho (o Mayús+clic): seleccionar · rueda: zoom",
        "{0}×   Linksklick: verschieben · Rechtsklick (oder Umschalt+Klick): auswählen · Mausrad: Zoom",
        "{0}×   clique esquerdo: mover · clique direito (ou Shift+clique): selecionar · roda: zoom",
        "{0}×   clic sinistro: sposta · clic destro (o Maiusc+clic): seleziona · rotella: zoom",
    ]),
    ("Choose a world then “Load”, or drop a schematic here", [
        "Choisis un monde puis « Charger », ou glisse un schematic ici",
        "Elige un mundo y luego «Cargar», o suelta un schematic aquí",
        "Wähle eine Welt und dann „Laden“, oder ziehe ein Schematic hierher",
        "Escolha um mundo e depois “Carregar”, ou arraste um schematic para cá",
        "Scegli un mondo e poi «Carica», o trascina qui uno schematic",
    ]),
    ("Rendering map… {0} regions left", [
        "Rendu de la carte… {0} régions restantes",
        "Dibujando el mapa… quedan {0} regiones",
        "Karte wird gezeichnet… noch {0} Regionen",
        "Desenhando o mapa… faltam {0} regiões",
        "Disegno della mappa… {0} regioni rimanenti",
    ]),
    // ── windows ──
    ("Go to", ["Aller à", "Ir a", "Gehe zu", "Ir para", "Vai a"]),
    ("Go", ["Aller", "Ir", "Los", "Ir", "Vai"]),
    ("Spawn point", ["Point d'apparition", "Punto de aparición", "Spawnpunkt", "Ponto de nascimento", "Punto di spawn"]),
    ("Selection", ["Sélection", "Selección", "Auswahl", "Seleção", "Selezione"]),
    ("Game resources (textures and models)", ["Ressources du jeu (textures et modèles)", "Recursos del juego (texturas y modelos)", "Spielressourcen (Texturen und Modelle)", "Recursos do jogo (texturas e modelos)", "Risorse del gioco (texture e modelli)"]),
    ("Automatic (world version)", ["Automatique (version du monde)", "Automático (versión del mundo)", "Automatisch (Weltversion)", "Automático (versão do mundo)", "Automatico (versione del mondo)"]),
    ("Choose another jar…", ["Choisir un autre jar…", "Elegir otro jar…", "Andere JAR wählen…", "Escolher outro jar…", "Scegli un altro jar…"]),
    ("Minecraft jar", ["Jar Minecraft", "Jar de Minecraft", "Minecraft-JAR", "Jar do Minecraft", "Jar di Minecraft"]),
    ("Current: {0}", ["En cours : {0}", "Actual: {0}", "Aktuell: {0}", "Atual: {0}", "Attuale: {0}"]),
    ("Resource packs (the first one has priority)", [
        "Packs de ressources (le premier est prioritaire)",
        "Paquetes de recursos (el primero tiene prioridad)",
        "Ressourcenpakete (das erste hat Vorrang)",
        "Pacotes de recursos (o primeiro tem prioridade)",
        "Pacchetti di risorse (il primo ha la priorità)",
    ]),
    ("Add a pack (.zip)…", ["Ajouter un pack (.zip)…", "Añadir un paquete (.zip)…", "Paket hinzufügen (.zip)…", "Adicionar um pacote (.zip)…", "Aggiungi un pacchetto (.zip)…"]),
    ("Resource pack", ["Pack de ressources", "Paquete de recursos", "Ressourcenpaket", "Pacote de recursos", "Pacchetto di risorse"]),
    ("Add a folder…", ["Ajouter un dossier…", "Añadir una carpeta…", "Ordner hinzufügen…", "Adicionar uma pasta…", "Aggiungi una cartella…"]),
    ("Apply and reload", ["Appliquer et recharger", "Aplicar y recargar", "Übernehmen und neu laden", "Aplicar e recarregar", "Applica e ricarica"]),
    ("Blocks to export", ["Blocs à exporter", "Bloques a exportar", "Zu exportierende Blöcke", "Blocos a exportar", "Blocchi da esportare"]),
    ("Blocks found in the loaded areas. Uncheck to exclude.", [
        "Blocs rencontrés dans les zones chargées. Décoche pour exclure.",
        "Bloques encontrados en las zonas cargadas. Desmarca para excluir.",
        "In den geladenen Bereichen gefundene Blöcke. Abwählen zum Ausschließen.",
        "Blocos encontrados nas áreas carregadas. Desmarque para excluir.",
        "Blocchi trovati nelle aree caricate. Deseleziona per escludere.",
    ]),
    ("Filter", ["Filtre", "Filtro", "Filter", "Filtro", "Filtro"]),
    ("Check all", ["Tout cocher", "Marcar todo", "Alle auswählen", "Marcar tudo", "Seleziona tutto"]),
    ("Uncheck all", ["Tout décocher", "Desmarcar todo", "Alle abwählen", "Desmarcar tudo", "Deseleziona tutto"]),
    // ── export progress and errors ──
    ("Reading chunks", ["Lecture des chunks", "Leyendo los chunks", "Chunks werden gelesen", "Lendo os chunks", "Lettura dei chunk"]),
    ("Preparing models", ["Préparation des modèles", "Preparando los modelos", "Modelle werden vorbereitet", "Preparando os modelos", "Preparazione dei modelli"]),
    ("Building mesh", ["Génération du maillage", "Generando la malla", "Netz wird erzeugt", "Gerando a malha", "Generazione della mesh"]),
    ("Entities", ["Entités", "Entidades", "Objekte", "Entidades", "Entità"]),
    ("Optimizing mesh", ["Optimisation du maillage", "Optimizando la malla", "Netz wird optimiert", "Otimizando a malha", "Ottimizzazione della mesh"]),
    ("Writing textures", ["Écriture des textures", "Escribiendo las texturas", "Texturen werden geschrieben", "Gravando as texturas", "Scrittura delle texture"]),
    ("Writing OBJ", ["Écriture de l'OBJ", "Escribiendo el OBJ", "OBJ wird geschrieben", "Gravando o OBJ", "Scrittura dell'OBJ"]),
    ("Export cancelled", ["Export annulé", "Exportación cancelada", "Export abgebrochen", "Exportação cancelada", "Esportazione annullata"]),
    ("level.dat not found in {0}", ["level.dat introuvable dans {0}", "No se encontró level.dat en {0}", "level.dat nicht gefunden in {0}", "level.dat não encontrado em {0}", "level.dat non trovato in {0}"]),
    ("The schematic contains no blocks", ["Le schematic ne contient aucun bloc", "El schematic no contiene ningún bloque", "Das Schematic enthält keine Blöcke", "O schematic não contém nenhum bloco", "Lo schematic non contiene blocchi"]),
    ("Unreadable NBT file", ["Fichier NBT illisible", "Archivo NBT ilegible", "NBT-Datei nicht lesbar", "Arquivo NBT ilegível", "File NBT illeggibile"]),
];

fn table() -> &'static HashMap<&'static str, [&'static str; 5]> {
    static T: OnceLock<HashMap<&'static str, [&'static str; 5]>> = OnceLock::new();
    T.get_or_init(|| TABLE.iter().map(|(k, v)| (*k, *v)).collect())
}

/// Translates an English text into the current language.
pub fn t(key: &str) -> String {
    let lang = current();
    if lang == Lang::En {
        return key.to_string();
    }
    match table().get(key) {
        Some(v) => v[lang as usize - 1].to_string(),
        None => key.to_string(),
    }
}

/// Translates then fills the {0}, {1}… placeholders.
pub fn tf(key: &str, args: &[&dyn std::fmt::Display]) -> String {
    let mut s = t(key);
    for (i, a) in args.iter().enumerate() {
        s = s.replace(&format!("{{{i}}}"), &a.to_string());
    }
    s
}
