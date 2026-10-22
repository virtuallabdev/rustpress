<script setup lang="ts">
import type { Actor, Content, ContentInput, ContentType, Field, Media, NavItem, Page, Revision } from '../../shared/types'
import { errorMessage } from '../../shared/types'

const actor = ref<Actor | null>(null)
const providers = ref<string[]>([])
const loading = ref(true)
const busy = ref(false)
const failure = ref('')
const notice = ref('')
const tab = ref('contents')
const types = ref<ContentType[]>([])
const contents = ref<Content[]>([])
const media = ref<Media[]>([])
const revisions = ref<Revision[]>([])
const offset = ref(0)
const search = ref('')
const selectedId = ref<string | null>(null)
const dirty = ref(false)
const draft = ref<ContentInput>(blank())
const navigationJson = ref('[]')
const typeSlug = ref('page')
const typeName = ref('Page')
const typeFields = ref('[]')
const alt = ref('')
const selectedFile = ref<File | null>(null)
const editorial = computed(() => actor.value?.role === 'admin' || actor.value?.role === 'editor')
const schema = computed<Field[]>(() => [
  ...(types.value.find(t => t.slug === draft.value.content_type)?.fields || []), ...draft.value.extra_fields,
])
const extraFieldsJson = ref('[]')

function blank(): ContentInput {
  return { content_type: 'page', slug: '', title: '', blocks: [], fields: {}, extra_fields: [],
    seo: { description: '', canonical: null, noindex: false }, status: 'draft',
    publish_at: new Date().toISOString(), version: null }
}
function inputOf(c: Content): ContentInput {
  return { content_type: c.content_type, slug: c.slug, title: c.title, blocks: c.blocks, fields: c.fields,
    extra_fields: c.extra_fields, seo: c.seo, status: c.status, publish_at: c.publish_at, version: c.version }
}
async function run(operation: () => Promise<void>) {
  if (busy.value) return
  busy.value = true
  failure.value = ''
  notice.value = ''
  try { await operation() } catch (error) { failure.value = errorMessage(error) } finally { busy.value = false }
}
function discardAllowed() { return !dirty.value || window.confirm('Scartare le modifiche non salvate?') }
async function loadList() {
  const data = await $fetch<Page<Content>>('/api/v1/contents', { query: { limit: 20, offset: offset.value, q: search.value } })
  contents.value = data.items
}
async function loadResources() {
  const [contentTypes, images, navigation] = await Promise.all([
    $fetch<ContentType[]>('/api/v1/types'),
    $fetch<Page<Media>>('/api/v1/media', { query: { limit: 100 } }),
    $fetch<NavItem[]>('/api/v1/public/navigation/main'),
  ])
  types.value = contentTypes
  media.value = images.items
  navigationJson.value = JSON.stringify(navigation, null, 2)
  loadType()
  await loadList()
}
function loadType() {
  const type = types.value.find(t => t.slug === typeSlug.value)
  typeName.value = type?.name || ''
  typeFields.value = JSON.stringify(type?.fields || [], null, 2)
}
async function edit(id: string) {
  if (!discardAllowed()) return
  await run(async () => {
    const content = await $fetch<Content>(`/api/v1/contents/${id}`)
    draft.value = inputOf(content)
    selectedId.value = id
    revisions.value = []
    extraFieldsJson.value = JSON.stringify(content.extra_fields, null, 2)
    dirty.value = false
  })
}
function newContent() {
  if (!discardAllowed()) return
  draft.value = blank()
  selectedId.value = null
  revisions.value = []
  extraFieldsJson.value = '[]'
  dirty.value = false
}
function parseArray(value: string): unknown[] {
  const parsed: unknown = JSON.parse(value)
  if (!Array.isArray(parsed)) throw new Error('Inserire un array JSON')
  return parsed
}
async function applyExtraFields() {
  await run(async () => {
    const values = parseArray(extraFieldsJson.value)
    const fields: Field[] = []
    for (const field of values) {
      if (!field || typeof field !== 'object' || !('key' in field) || typeof field.key !== 'string'
        || !('label' in field) || typeof field.label !== 'string' || !('kind' in field)
        || !['text', 'number', 'boolean', 'date', 'media', 'relation'].some(k => k === field.kind)
        || ('required' in field && typeof field.required !== 'boolean')) throw new Error('Definizione campo non valida')
      const kind = field.kind
      if (kind !== 'text' && kind !== 'number' && kind !== 'boolean' && kind !== 'date' && kind !== 'media' && kind !== 'relation') throw new Error('Tipo campo non valido')
      fields.push({ key: field.key, label: field.label, kind, required: 'required' in field && field.required === true })
    }
    draft.value.extra_fields = fields
    dirty.value = true
    notice.value = 'Schema applicato localmente. Compila i campi e salva il contenuto.'
  })
}
async function save() {
  await run(async () => {
    const extraFields = actor.value?.role === 'admin' ? parseArray(extraFieldsJson.value) : draft.value.extra_fields
    const body = { ...draft.value, extra_fields: extraFields }
    const saved = selectedId.value
      ? await $fetch<Content>(`/api/v1/contents/${selectedId.value}`, { method: 'PUT', body })
      : await $fetch<Content>('/api/v1/contents', { method: 'POST', body })
    draft.value = inputOf(saved)
    selectedId.value = saved.id
    dirty.value = false
    notice.value = `Salvato: revisione ${saved.version}.`
    await loadList()
  })
}
async function showRevisions() {
  if (!selectedId.value) return
  await run(async () => {
    const data = await $fetch<Page<Revision>>(`/api/v1/contents/${selectedId.value}/revisions`)
    revisions.value = data.items
  })
}
function restore(revision: Revision) {
  if (!discardAllowed()) return
  const currentVersion = draft.value.version
  draft.value = { ...inputOf(revision.snapshot), status: 'draft', version: currentVersion }
  extraFieldsJson.value = JSON.stringify(draft.value.extra_fields, null, 2)
  dirty.value = true
  notice.value = 'Revisione caricata come bozza: controlla e salva per confermare il ripristino.'
}
async function saveType() {
  await run(async () => {
    await $fetch(`/api/v1/types/${encodeURIComponent(typeSlug.value)}`, { method: 'PUT',
      body: { name: typeName.value, fields: parseArray(typeFields.value) } })
    types.value = await $fetch<ContentType[]>('/api/v1/types')
    notice.value = 'Schema salvato.'
  })
}
async function saveNavigation() {
  await run(async () => {
    await $fetch('/api/v1/navigation/main', { method: 'PUT', body: parseArray(navigationJson.value) })
    notice.value = 'Navigazione salvata.'
  })
}
function selectFile(event: Event) {
  if (event.target instanceof HTMLInputElement) selectedFile.value = event.target.files?.[0] || null
}
async function upload() {
  await run(async () => {
    if (!selectedFile.value) throw new Error('Seleziona un file immagine')
    const form = new FormData()
    form.append('file', selectedFile.value)
    form.append('alt', alt.value)
    const uploaded = await $fetch<Media>('/api/v1/media', { method: 'POST', body: form })
    media.value.unshift(uploaded)
    notice.value = `Immagine caricata: ${uploaded.id}. Le varianti sono pubbliche.`
  })
}
async function logout() {
  if (!discardAllowed()) return
  await run(async () => {
    await $fetch('/api/v1/auth/logout', { method: 'POST' })
    actor.value = null
    contents.value = []
    media.value = []
    types.value = []
    dirty.value = false
    newContent()
  })
}
onMounted(async () => {
  await run(async () => {
    providers.value = await $fetch<string[]>('/api/v1/auth/providers')
    try { actor.value = await $fetch<Actor>('/api/v1/auth/me') } catch (error) {
      if (!(error && typeof error === 'object' && 'statusCode' in error && error.statusCode === 401)) throw error
    }
    if (actor.value) await loadResources()
  })
  loading.value = false
})
function beforeUnload(event: BeforeUnloadEvent) {
  if (dirty.value) { event.preventDefault(); event.returnValue = '' }
}
onMounted(() => window.addEventListener('beforeunload', beforeUnload))
onUnmounted(() => window.removeEventListener('beforeunload', beforeUnload))
</script>

<template>
  <NuxtRouteAnnouncer />
  <a class="btn btn-primary absolute -top-20 focus:top-2" href="#main">Vai al contenuto</a>
  <header class="navbar bg-base-200 px-6 gap-4">
    <span class="text-xl font-bold flex-1">RustPress <span class="badge">0.1</span></span>
    <span v-if="actor">{{ actor.role }}</span>
    <button v-if="actor" class="btn btn-ghost" :disabled="busy" @click="logout">Esci</button>
  </header>
  <main id="main" class="max-w-7xl mx-auto p-6">
    <div v-if="failure" role="alert" class="alert alert-error mb-4">{{ failure }}</div>
    <div v-if="notice" role="status" class="alert alert-success mb-4">{{ notice }}</div>
    <p v-if="loading" role="status">Caricamento...</p>
    <section v-else-if="!actor" class="card bg-base-200 max-w-xl mx-auto my-12">
      <div class="card-body">
        <h1 class="card-title">Accedi all'amministrazione</h1>
        <p>Accesso riservato alle identita autorizzate dall'operatore.</p>
        <a v-for="provider in providers" :key="provider" class="btn btn-primary mt-2" :href="`/api/v1/auth/${provider}/login`">Accedi con {{ provider }}</a>
        <p v-if="!providers.length" class="alert alert-warning">Nessun provider configurato. Consulta la guida di installazione.</p>
      </div>
    </section>
    <template v-else>
      <nav aria-label="Amministrazione" class="flex flex-wrap gap-2 mb-6">
        <button v-for="item in ['contents', 'media', ...(actor.role === 'admin' ? ['schemas'] : []), ...(editorial ? ['navigation'] : [])]" :key="item"
          class="btn" :class="{ 'btn-primary': tab === item }" :aria-current="tab === item ? 'page' : undefined" @click="tab = item">
          {{ ({ contents: 'Contenuti', media: 'Media', schemas: 'Schemi', navigation: 'Navigazione' })[item] }}
        </button>
      </nav>
      <section v-if="tab === 'contents'" class="grid lg:grid-cols-[20rem_1fr] gap-8">
        <aside>
          <h1 class="text-2xl font-bold mb-4">Contenuti</h1>
          <button class="btn btn-primary mb-4" :disabled="busy" @click="newContent">Nuovo contenuto</button>
          <form class="flex gap-2 mb-4" @submit.prevent="run(async () => { offset = 0; await loadList() })">
            <input v-model="search" aria-label="Cerca contenuti" type="search" class="input w-full" maxlength="200">
            <button class="btn" :disabled="busy">Cerca</button>
          </form>
          <ul class="menu w-full p-0">
            <li v-for="content in contents" :key="content.id"><button :disabled="busy" @click="edit(content.id)">
              <span>{{ content.title }}</span><span class="badge badge-outline">{{ content.status }}</span>
            </button></li>
          </ul>
          <div class="flex gap-2 mt-4">
            <button class="btn btn-sm" :disabled="busy || offset === 0" @click="run(async () => { offset -= 20; await loadList() })">Precedenti</button>
            <button class="btn btn-sm" :disabled="busy || contents.length < 20" @click="run(async () => { offset += 20; await loadList() })">Successivi</button>
          </div>
        </aside>
        <form class="space-y-5" @submit.prevent="save" @input="dirty = true" @change="dirty = true">
          <h2 class="text-2xl font-bold">{{ selectedId ? 'Modifica' : 'Crea' }} <span v-if="dirty" class="badge badge-warning">Non salvato</span></h2>
          <div class="grid sm:grid-cols-2 gap-4">
            <label class="fieldset">Tipo<select v-model="draft.content_type" class="select w-full" :disabled="!!selectedId"><option v-for="type in types" :key="type.slug" :value="type.slug">{{ type.name }}</option></select></label>
            <label class="fieldset">Slug<input v-model="draft.slug" class="input w-full" required maxlength="120" pattern="[a-z0-9]+(-[a-z0-9]+)*"></label>
          </div>
          <label class="fieldset">Titolo<input v-model="draft.title" class="input w-full" required maxlength="300"></label>
          <fieldset class="space-y-4">
            <legend class="font-semibold mb-3">Blocchi</legend>
            <div v-for="(block, index) in draft.blocks" :key="index" class="card border border-base-300 p-4 gap-3">
              <div class="flex gap-2 items-center"><span class="badge flex-1">{{ block.kind }}</span>
                <button type="button" class="btn btn-sm" :disabled="index === 0" :aria-label="`Sposta blocco ${index + 1} in alto`" @click="draft.blocks.splice(index - 1, 0, ...draft.blocks.splice(index, 1)); dirty = true">Su</button>
                <button type="button" class="btn btn-sm btn-error" :aria-label="`Rimuovi blocco ${index + 1}`" @click="draft.blocks.splice(index, 1); dirty = true">Rimuovi</button>
              </div>
              <label v-if="block.kind !== 'image'" class="fieldset">Testo<textarea v-model="block.text" class="textarea w-full" rows="4"></textarea></label>
              <label v-if="block.kind === 'heading'" class="fieldset">Livello<select v-model.number="block.level" class="select"><option v-for="level in [2,3,4,5,6]" :key="level" :value="level">H{{ level }}</option></select></label>
              <template v-if="block.kind === 'image'">
                <label class="fieldset">Immagine<select v-model="block.media_id" class="select w-full" required><option value="" disabled>Seleziona</option><option v-for="img in media" :key="img.id" :value="img.id">{{ img.alt }}</option></select></label>
                <label class="fieldset">Testo alternativo<input v-model="block.alt" class="input w-full" maxlength="500"></label>
              </template>
            </div>
            <div class="flex flex-wrap gap-2">
              <button type="button" class="btn btn-sm" @click="draft.blocks.push({ kind: 'paragraph', text: '' }); dirty = true">+ Paragrafo</button>
              <button type="button" class="btn btn-sm" @click="draft.blocks.push({ kind: 'heading', text: '', level: 2 }); dirty = true">+ Titolo</button>
              <button type="button" class="btn btn-sm" @click="draft.blocks.push({ kind: 'image', media_id: '', alt: '' }); dirty = true">+ Immagine</button>
            </div>
          </fieldset>
          <fieldset v-if="schema.length" class="grid sm:grid-cols-2 gap-4">
            <legend class="font-semibold mb-3">Campi personalizzati (pubblici alla pubblicazione)</legend>
            <label v-for="field in schema" :key="field.key" class="fieldset">{{ field.label }} {{ field.required ? '*' : '' }}
              <input v-if="field.kind === 'boolean'" v-model="draft.fields[field.key]" type="checkbox" class="checkbox" :required="false">
              <input v-else-if="field.kind === 'number'" v-model.number="draft.fields[field.key]" type="number" step="any" class="input w-full" :required="field.required">
              <input v-else v-model="draft.fields[field.key]" :type="field.kind === 'date' ? 'date' : 'text'" class="input w-full" :required="field.required" :placeholder="['media','relation'].includes(field.kind) ? 'UUID' : ''">
            </label>
          </fieldset>
          <details v-if="actor.role === 'admin'" class="collapse collapse-arrow border border-base-300">
            <summary class="collapse-title">Schema aggiuntivo di questa pagina/post</summary>
            <div class="collapse-content"><label class="fieldset">Array JSON dei campi aggiuntivi<textarea v-model="extraFieldsJson" class="textarea w-full font-mono" rows="6"></textarea></label><button type="button" class="btn btn-sm mt-2" :disabled="busy" @click="applyExtraFields">Applica schema al modulo</button><p>Applica, compila i nuovi campi e salva per rendere persistenti le modifiche.</p></div>
          </details>
          <fieldset class="space-y-3">
            <legend class="font-semibold">SEO e GEO</legend>
            <label class="fieldset">Descrizione<textarea v-model="draft.seo.description" class="textarea w-full" maxlength="500"></textarea></label>
            <label class="fieldset">Canonical opzionale<input :value="draft.seo.canonical || ''" class="input w-full" @input="draft.seo.canonical = ($event.target as HTMLInputElement).value || null"></label>
            <label class="flex gap-2"><input v-model="draft.seo.noindex" type="checkbox" class="checkbox">Escludi dall'indicizzazione</label>
          </fieldset>
          <label class="fieldset">Pubblicazione (ISO 8601 con fuso, es. 2026-12-01T09:00:00Z)<input v-model="draft.publish_at" class="input w-full" required></label>
          <label class="fieldset">Stato<select v-model="draft.status" class="select" :disabled="!editorial"><option value="draft">Bozza</option><option v-if="editorial" value="published">Pubblicato / programmato</option><option v-if="editorial" value="archived">Archiviato</option></select></label>
          <div class="flex gap-3">
            <button class="btn btn-primary" :disabled="busy">Salva</button>
            <button v-if="selectedId" type="button" class="btn" :disabled="busy" @click="showRevisions">Revisioni</button>
          </div>
          <ul v-if="revisions.length" class="menu border border-base-300 rounded-box">
            <li v-for="revision in revisions" :key="revision.id"><button type="button" @click="restore(revision)">Carica revisione {{ revision.version }} — {{ revision.created_at }}</button></li>
          </ul>
        </form>
      </section>
      <section v-if="tab === 'media'" class="space-y-6">
        <h1 class="text-2xl font-bold">Libreria media</h1>
        <p>JPEG, PNG o WebP; richiesta massima 10 MiB, 25 megapixel. Le varianti caricate sono pubbliche, anche se non usate in contenuti pubblicati.</p>
        <form class="flex flex-wrap items-end gap-4" @submit.prevent="upload">
          <label class="fieldset">Immagine<input type="file" accept="image/jpeg,image/png,image/webp" class="file-input" required @change="selectFile"></label>
          <label class="fieldset">Testo alternativo<input v-model="alt" class="input" required maxlength="500"></label>
          <button class="btn btn-primary" :disabled="busy">Carica e ridimensiona</button>
        </form>
        <div class="grid sm:grid-cols-3 gap-4">
          <figure v-for="img in media" :key="img.id" class="card border border-base-300 p-4">
            <img :src="img.variants[0]?.url" :alt="img.alt" loading="lazy" class="h-40 object-contain">
            <figcaption class="mt-2 break-all">{{ img.alt }}<br><code class="text-xs">{{ img.id }}</code></figcaption>
          </figure>
        </div>
        <p>Visualizzati gli ultimi 100 media. L'API supporta la paginazione.</p>
      </section>
      <section v-if="tab === 'schemas' && actor.role === 'admin'" class="max-w-3xl space-y-4">
        <h1 class="text-2xl font-bold">Tipi contenuto e schemi</h1>
        <p>Campi: key, label, kind (text, number, boolean, date, media, relation), required. Le modifiche incompatibili con contenuti esistenti vengono rifiutate.</p>
        <label class="fieldset">Tipo esistente<select v-model="typeSlug" class="select" @change="loadType"><option v-for="type in types" :key="type.slug" :value="type.slug">{{ type.name }}</option></select></label>
        <form class="space-y-4" @submit.prevent="saveType">
          <label class="fieldset">Slug tipo (nuovo o esistente)<input v-model="typeSlug" class="input" required></label>
          <label class="fieldset">Nome<input v-model="typeName" class="input" required></label>
          <label class="fieldset">Definizioni campi JSON<textarea v-model="typeFields" class="textarea w-full font-mono" rows="12" spellcheck="false"></textarea></label>
          <pre class="mockup-code p-4 whitespace-pre-wrap">[{"key":"subtitle","label":"Sottotitolo","kind":"text","required":false}]</pre>
          <button class="btn btn-primary" :disabled="busy">Salva schema</button>
        </form>
      </section>
      <section v-if="tab === 'navigation' && editorial" class="max-w-3xl">
        <h1 class="text-2xl font-bold mb-4">Navigazione principale</h1>
        <form class="space-y-4" @submit.prevent="saveNavigation">
          <label class="fieldset">Collegamenti JSON<textarea v-model="navigationJson" class="textarea w-full font-mono" rows="12"></textarea></label>
          <pre class="mockup-code p-4 whitespace-pre-wrap">[{"label":"Home","href":"/","children":[]}]</pre>
          <button class="btn btn-primary" :disabled="busy">Salva navigazione</button>
        </form>
      </section>
    </template>
  </main>
</template>
