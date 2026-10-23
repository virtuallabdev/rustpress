# RustPress

CMS single-site e base di framework headless, scritto in Rust con API Axum,
PostgreSQL, amministrazione Nuxt e tema SSR Nuxt con daisyUI.

**Stato: prototipo funzionale 0.1, non una sostituzione completa di WordPress
e non ancora pronto per la produzione.** Le funzioni implementate sono elencate
qui sotto; la parita con WordPress e un obiettivo successivo, non una proprieta
gia raggiunta. Nessuna garanzia di sicurezza assoluta, prestazioni massime o
posizionamento nei motori di ricerca/AI.

## Stack fissato

| Componente | Versione |
| --- | --- |
| Rust | 1.99.1 |
| Axum | 0.8.9 |
| SQLx | 0.9.0 |
| PostgreSQL di riferimento | 18.6 |
| Nuxt | 4.6.2 |
| Vue | 3.5.44 |
| daisyUI | 5.7.50 |
| Tailwind CSS | 4.3.3 |
| Node per i container | 26.2.0 |
| TypeScript | 6.0.3 |
| Nuxt i18n | 10.6.0 |
| Tiptap / Markdown | 3.32.0 |

Versioni verificate il 22 ottobre 2026: PostgreSQL 19 era ancora una release
candidate; e stato scelto PostgreSQL 18.6 stabile, non una prerelease.
TypeScript 6 e fissato anche nelle dipendenze transitive: `vue-tsc` 3.3.13
non funziona con l'API del compilatore TypeScript 7.
Conservare entrambi i lockfile. Le immagini container sono fissate per digest.

## Funzioni disponibili

- Pagine, post e tipi contenuto configurabili; slug univoci per lingua.
- Interfacce italiana/inglese, traduzioni collegate, menu per lingua, URL e
  hreflang delle sole traduzioni pubblicate. Nessun fallback di contenuto implicito.
- Editor Tiptap con sorgente Markdown, modalita visuale e anteprima sicura:
  titoli, grassetto/corsivo, liste, citazioni, codice, tabelle, link e immagini.
  I vecchi blocchi restano leggibili e convertibili esplicitamente.
- Schemi di campi per tipo e aggiunte per singolo documento: testo, numero,
  booleano, data, riferimento media e relazione a contenuto.
  Validazione server-side, rifiuto dei campi sconosciuti e dei cambi schema
  incompatibili con documenti esistenti.
- Bozze, pubblicazione, pubblicazione programmata tramite data UTC e archivio.
  Un documento programmato diventa visibile alla prima lettura successiva
  alla scadenza, senza dipendere da cron.
- Revisioni persistenti a ogni salvataggio, ripristino da UI come nuova bozza,
  controllo concorrenza ottimistico: una versione obsoleta restituisce 409.
- Ricerca full-text PostgreSQL, elenchi paginati e navigazione a tre livelli.
- Upload JPEG/PNG/WebP, limiti di byte/pixel/memoria, correzione orientamento,
  resize proporzionale senza upscaling e varianti WebP fino a 1920 px.
  Re-encoding senza trasferire EXIF; originali separati dalle directory pubbliche.
- Descrizione SEO, canonical, noindex, Open Graph, sitemap, robots e JSON-LD
  Article/WebPage. GEO significa qui Generative Engine Optimization:
  contenuti SSR e dati strutturati accessibili, non geolocalizzazione.
- OAuth server-side Google, Microsoft e Facebook Login (Meta), con identita
  autorizzate esplicitamente dall'operatore, sessioni opache e ruoli.
- API versionata, contratto OpenAPI e istruzioni per agenti AI; credenziali
  agent separate, con scadenza, limitate alla preparazione di bozze.
- Audit delle scritture privilegiate, migrazioni SQL e container multi-stage
  non-root per API e frontend.

Alcuni strumenti amministrativi, come schemi e menu, usano un editor JSON
validato dal backend: non sono ancora visual builder.

## Struttura

- [src](src): API, autorizzazione, OAuth, validazione e trasformazioni immagini.
- [migrations](migrations): schema PostgreSQL e dati iniziali non sensibili.
- [web/admin](web/admin): amministrazione Nuxt, origine separata.
- [web/theme](web/theme): tema pubblico SSR; proxy limitato alle API pubbliche.
- [web/shared](web/shared): tipi condivisi tra le interfacce.
- [openapi.json](openapi.json): contratto API per integrazioni.
- [docs/agents.txt](docs/agents.txt): regole per agenti AI.
- [tests](tests) e [scripts](scripts): verifiche Rust, PostgreSQL e full-stack.

## Avvio locale con Docker

Richiede Docker Engine/Desktop e Docker Compose v2 o successiva.
Su questa macchina Docker e disponibile attraverso WSL Ubuntu:
usare `wsl --exec docker` al posto di `docker` se il comando Windows non esiste.
Le variabili per Compose devono essere disponibili nel processo WSL; non
presumere che le variabili PowerShell siano inoltrate automaticamente.

Da PowerShell, assegnare una password generata, URL-safe, nella sessione:

```powershell
$env:POSTGRES_PASSWORD = '<GENERATED_URL_SAFE_PASSWORD>'
docker compose up --build -d
docker compose ps
```

Tema: `http://localhost:3000`. Amministrazione: `http://localhost:3001`.
Database e API non pubblicano porte host. I volumi `postgres` e `media` sono
persistenti: non eliminarli durante aggiornamenti o arresti ordinari.

Lo stack e **solo per sviluppo locale**: cookie non-Secure, origini localhost.
Non esporlo a Internet invariato. Vedere [operazioni](docs/operations.md).

Su questo host i registry container e npm sono raggiungibili, ma i download
Rust da WSL/container vanno in timeout. La build Linux dell'API e stata quindi
verificata offline con dipendenze vendorizzate dal lockfile, senza disabilitare
TLS; le due immagini Nuxt sono state costruite normalmente. Il Dockerfile API
distribuito usa invece Cargo online e richiede accesso a crates.io.

## Avvio nativo

Richiede Rust 1.99.1, Node compatibile con Nuxt e un database PostgreSQL
dedicato UTF-8. Il processo Rust non carica automaticamente file di ambiente:
esportare le variabili nella sessione del processo o usare il secret manager
del proprio deployment. Non versionare segreti.

```powershell
$env:DATABASE_URL = 'postgres://<USER>:<URL_ENCODED_PASSWORD>@127.0.0.1:5432/<DATABASE>'
$env:RUSTPRESS_ENV = 'development'
$env:ADMIN_ORIGIN = 'http://localhost:3001'
$env:SITE_ORIGIN = 'http://localhost:3000'
$env:NUXT_API_INTERNAL = 'http://127.0.0.1:8080'
$env:NUXT_PUBLIC_SITE_ORIGIN = 'http://localhost:3000'
cargo run --locked
```

In due terminali separati, dopo `npm ci`:

```powershell
npm run dev:admin
# Nell'altro terminale:
npm run dev:theme
```

Le migrazioni vengono applicate all'avvio. `cargo run --locked -- migrate`
esegue solo le migrazioni; usare un unico processo migratore negli ambienti
gestiti e applicare backup prima delle modifiche allo schema.

| Variabile | Significato |
| --- | --- |
| `DATABASE_URL` | Connessione PostgreSQL, obbligatoria |
| `RUSTPRESS_ENV` | `production` predefinito, oppure `development` |
| `ADMIN_ORIGIN` | Origine amministrazione senza percorsi, obbligatoria |
| `SITE_ORIGIN` | Origine sito senza percorsi, obbligatoria |
| `BIND_ADDR` | Bind API, predefinito `127.0.0.1:8080` |
| `MEDIA_DIR` | Directory persistente, predefinita `data/media` |
| `RUST_LOG` | Filtro logging, predefinito `rustpress=info` |
| `NUXT_API_INTERNAL` | API raggiungibile dai processi Nuxt, mai dal bundle client |
| `NUXT_PUBLIC_SITE_ORIGIN` | Origine pubblica usata dal tema per canonical e robots |

### Login e primo amministratore

Registrare applicazioni OAuth nei portali dei provider. Usare coppie complete
`GOOGLE_CLIENT_ID` / `GOOGLE_CLIENT_SECRET`,
`MICROSOFT_CLIENT_ID` / `MICROSOFT_CLIENT_SECRET`,
`META_CLIENT_ID` / `META_CLIENT_SECRET`.
Una coppia vuota disabilita il provider; una coppia incompleta impedisce l'avvio.
Microsoft richiede anche `MICROSOFT_TENANT_ID`, UUID di un tenant esplicito:
questa versione non abilita tenant arbitrari o account Microsoft personali.
Meta usa Facebook Login Graph API v24.0; produzione e redirect localhost
dipendono dalle regole e dall'approvazione della propria app Meta.

Registrare i redirect esatti:

```text
<ADMIN_ORIGIN>/api/v1/auth/google/callback
<ADMIN_ORIGIN>/api/v1/auth/microsoft/callback
<ADMIN_ORIGIN>/api/v1/auth/meta/callback
```

L'operatore deve conoscere l'identificatore stabile del provider per l'utente:
Google `sub`, Microsoft Graph `id`, Meta app-scoped `id`. Non usare l'email
come identificatore e non condividere account di app diverse.

```powershell
cargo run --locked -- grant google '<PROVIDER_SUBJECT>' admin
# In Docker, con le variabili OAuth esportate prima dell'avvio:
docker compose exec api rustpress grant google '<PROVIDER_SUBJECT>' admin
```

Ruoli assegnabili: `admin`, `editor`, `author`. Non esiste registrazione libera.
Non ci sono password locali o account amministratore di default.
L'operatore con accesso al database puo revocare un'identita e le sue sessioni;
il privilegio del ruolo viene riletto a ogni richiesta.

**I flussi OAuth con account reali non sono stati verificati:** richiedono
app registrate, secret e consenso del proprietario. I test locali coprono
sessioni, autorizzazione, CSRF e PKCE, non l'accettazione dei singoli provider.

### Agenti AI

```powershell
cargo run --locked -- agent-token '<AGENT_LABEL>'
```

Il comando consegna una volta il token su stdout: eseguirlo in un terminale
privato, senza registrazione della sessione. Conservare il valore in un secret
manager e passarlo nell'header `Authorization: Bearer <TOKEN>`.
Scadenza: 30 giorni. Il database conserva solo SHA-256 del token.
`revoke-token <SHA256>` revoca una credenziale conosciuta.
Non inviare token a servizi AI esterni.

L'API interna deve essere raggiunta attraverso un gateway fidato o la rete
privata; il tema pubblico non espone il proxy editoriale. Contratto disponibile
all'amministrazione su `/api/v1/openapi.json`.

| Ruolo | Contenuti | Pubblicazione/archivio | Schemi | Menu | Upload |
| --- | --- | --- | --- | --- | --- |
| admin | Tutti | Si | Si | Si | Si |
| editor | Tutti | Si | No | Si | Si |
| author | Solo propri, modifica solo bozze | No | No | No | Si |
| agent | Tutte le bozze leggibili, modifica solo bozze | No | No | No | No |

Gli agenti hanno visibilita editoriale ampia in questa versione single-site:
emettere token solo ad agenti fidati. Non sono ancora disponibili scope per
documento, isolamento tenant, MCP, CLI editoriale o webhook.

## Verifica

```powershell
cargo fmt --all --check
cargo test --all-targets --locked
cargo clippy --all-targets --locked -- -D warnings
npm ci
npm run typecheck
npm run build
npm run test:integration
npm run test:e2e
npx playwright install chromium
npm run test:browser
```

Il test PostgreSQL e marcato `ignored` nel normale `cargo test`: il runner npm
avvia un cluster temporaneo PostgreSQL 18.4 locale, non usa un database esistente,
esegue le migrazioni e il test esplicito, poi arresta ed elimina il cluster.
La versione embedded e una dipendenza **solo di sviluppo**, non il DB distribuito.
Il test full-stack richiede prima `npm run build`; usa le due build SSR e
l'eseguibile Rust prodotto dai test.

Il test full-stack controlla creazione e pubblicazione reali tramite proxy,
isolamento API pubbliche, cookie/Origin, escaping XSS, canonical, hreflang, lingue
e sitemap. `test:browser` aggiunge Chromium con back office autenticato, cambio
lingua, roundtrip sorgente/visuale, anteprima, pubblicazione e bozza tradotta.
Non equivale a un audit di sicurezza o a un load test.

Verifiche eseguite il 23 ottobre 2026:

- 17 test Rust unitari/HTTP, inclusi Markdown, limiti UTF-8 e URL non sicuri.
- Test d'integrazione su PostgreSQL 18.4 superato: migrazioni, ciclo editoriale,
  concorrenza, ruoli, audit, upload multipart, resize, distribuzione WebP,
  migrazione dei vecchi documenti/revisioni e traduzioni collegate.
- Typecheck e build di produzione di entrambe le applicazioni Nuxt superati.
- Smoke test full-stack superato con processi reali, poi arrestati.
- Apertura nel browser della schermata di accesso e di una pagina SSR:
  i payload HTML di prova risultano testo, non elementi eseguibili.
  Superato anche il test Chromium del back office autenticato con roundtrip
  di titoli, enfasi, liste, tabelle e codice Markdown.
- Build Linux delle tre immagini e smoke test Docker tramite WSL superati
  con PostgreSQL 18.6, migrazioni, proxy, SSR inglese e processi applicativi
  non-root con filesystem read-only. I container temporanei sono stati rimossi.

Lo smoke test container Windows/WSL richiede le immagini con tag di verifica:

```powershell
wsl --exec docker build -t rustpress-api:verify .
wsl --exec docker build -f web/Dockerfile --build-arg APP=admin -t rustpress-admin:verify .
wsl --exec docker build -f web/Dockerfile --build-arg APP=theme -t rustpress-theme:verify .
.\scripts\test-containers.ps1
```

La prima build richiede la rete Rust descritta sopra. Lo script crea un
database e una rete isolati, non usa i volumi del sito e rimuove solo le
risorse temporanee che ha creato.

## Traduzioni

La lingua dell'interfaccia amministrativa e indipendente dalla lingua del
documento. La preferenza UI viene mantenuta in un cookie funzionale.
Italiano e inglese sono le due lingue abilitate; aggiungerne altre richiede
aggiornare insieme cataloghi, configurazione Nuxt, validazione e vincoli DB.
Le stringhe UI sono cataloghi condivisi con verifica TypeScript della parita
delle chiavi. Gli errori del protocollo API restano in inglese.

Nel documento salvato usare **Crea traduzione** per preparare una bozza
collegata: titolo, corpo, campi e slug vanno tradotti editorialmente, non tramite
un servizio esterno automatico. Il canonical e la descrizione SEO della copia
vengono azzerati; nessuna pubblicazione automatica. Ogni versione linguistica
ha revisioni e pubblicazione indipendenti.

L'API accetta `locale=it|en`. Gli elenchi editoriali senza filtro includono
tutte le lingue; le letture pubbliche e i menu senza filtro usano `it`.
`translation_of` e l'UUID del documento originale, ammesso solo in POST.
Lingua e gruppo non si possono cambiare dopo la creazione; un gruppo ammette
un documento per lingua. La creazione di traduzioni non aggira i permessi autore.
Lo schema per-documento viene ereditato dalla sorgente anche per editor e
autori autorizzati; solo un amministratore puo cambiarne le definizioni.
Le traduzioni non pubblicate/programmate non compaiono nei link pubblici.

URL italiani esistenti mantenuti: `/<slug>`. URL inglesi: `/en/<slug-inglese>`.
Una traduzione inesistente restituisce 404, non contenuto di un'altra lingua.
Il menu inglese e indipendente e inizialmente vuoto. Il cambio lingua del
contenuto usa gli slug realmente collegati; i link lingua dell'header portano
alle rispettive home page.

**Aggiornamento di siti esistenti:** applicare la migrazione 0002 dopo un backup.
I documenti esistenti diventano italiani, ciascuno nel proprio gruppo, senza
modifiche al corpo o agli snapshot delle revisioni. Uno slug italiano `en`
confligge con la nuova home inglese: rinominarlo prima dell'aggiornamento.
La migrazione rifiuta esplicitamente questo conflitto anziche nascondere la pagina.

## Editor Markdown e scelta del back office

Sono stati verificati [Nuxt UI Editor](https://ui.nuxt.com/docs/components/editor)
e [Tiptap Markdown](https://tiptap.dev/docs/editor/markdown/getting-started/basic-usage).
Nuxt UI offre gia un editor basato su Tiptap, ma introdurre un secondo sistema
di componenti insieme a daisyUI non porta un vantaggio necessario.
E stato integrato direttamente Tiptap Vue 3 con la sua estensione Markdown
ufficiale e i controlli daisyUI.

Il database salva il **testo Markdown**, non l'HTML generato dall'editor.
Il limite e 500.000 byte UTF-8 per documento; con Markdown presente `blocks`
deve essere vuoto. Le revisioni comprendono sorgente e lingua.
Le letture pubbliche includono `body_html`, prodotto esclusivamente dal renderer
Rust con `pulldown-cmark` e allowlist `ammonia`; l'anteprima autenticata usa lo
stesso percorso. HTML grezzo viene trattato come testo, link pericolosi filtrati
e immagini remote escluse: sono ammesse solo varianti WebP locali generate.

**Markdown da solo non impedisce XSS.** La protezione dipende dal rendering:
non renderizzare altrove il sorgente con HTML libero o plugin non fidati.
La modalita visuale viene inizializzata con HTML gia sanitizzato, non con un
parser permissivo di HTML incorporato nel Markdown originale.
Il sorgente viene mantenuto esatto finche non viene modificato; l'editing visuale
puo normalizzare formattazione/spazi Markdown. Non e un editor MDX, non esegue
Vue/JavaScript, non consente HTML personalizzato o estensioni arbitrarie.
I blocchi preesistenti sono convertiti solo su comando esplicito; salvando si
crea una nuova revisione e resta possibile recuperare il precedente formato.

**Decisione Leptos:** il back office attivo resta Nuxt. Un client Leptos/WASM
non sostituisce autorizzazione server, CSRF, sessioni e sanitizzazione; anche
WASM opera nel browser e puo introdurre XSS tramite DOM/HTML non sicuro.
Non ci sono benchmark o evidenze che una seconda UI migliori qui sicurezza o
costi. Duplicare il back office aumenterebbe codice e test da mantenere e
richiederebbe integrare comunque un editor browser. Per questo non e stata
creata una seconda interfaccia: la condizione di convenienza richiesta non
e soddisfatta. La decisione puo essere rivalutata con requisiti e misure concrete.

## Limiti e prossime decisioni

Non sono ancora implementati:

- Plugin/temi installabili e loro sandbox, importazione o compatibilita WordPress.
- Tassonomie, gruppi ripetibili, blocchi annidati e template con schema dedicato.
- Autosalvataggio e anteprima privata condivisibile; la UI segnala
  modifiche non salvate, ma non le salva automaticamente.
- Multisito, multitenancy, traduzione automatica e ACL granulari.
- Commenti, moderazione, form pubblici, ecommerce, iscrizioni e newsletter.
- Redirect SEO, social image dedicata, sitemap indicizzata oltre 50.000 URL.
- Crop/focal point, AVIF, SVG, video, documenti, antivirus, storage S3 e CDN.
- MFA/passkey gestite dal CMS, inviti self-service e gestione utenti da UI.
- MCP, webhook, job queue, caching distribuito e benchmark prestazionali.
- Backup automatici, ripristino verificato e pipeline CI/CD.

Per GEO e fornito solo il fondamento tecnico: non sono presenti metriche di
citazione AI ne meccanismi per garantire visibilita. Le impostazioni di default
del questionario rimasto senza risposte sono ipotesi, non requisiti approvati.

La licenza del progetto deve essere scelta dal proprietario prima della
distribuzione; non e stata assegnata automaticamente una licenza open source.

## Blocchi al rilascio

`npm audit` rileva advisory high transitivi di Nuxt relativi a `braces <=3.0.3`
e `node-forge <=1.4.0`; le ultime versioni consultate non contenevano una patch.
L'audit contava 11 pacchetti interessati, non 11 vulnerabilita indipendenti.
Non e stato forzato un downgrade di Nuxt ne mascherato l'audit con override
non verificati. Ripetere l'audit, aggiornare e verificare prima di distribuire.

Restano obbligatori: riprova della build API online nella rete di deployment,
test OAuth reali, controllo di sicurezza dedicato, test accessibilita, benchmark
su hardware definito e prova di ripristino. Vedere [operazioni](docs/operations.md).
