<script setup lang="ts">
import type { Actor, Content, ContentInput, ContentType, Field, Media, NavItem, Page, Revision } from '../../shared/types'
import { errorMessage } from '../../shared/types'

const { t, locale, setLocale } = useI18n()
const languageCookie = useCookie<'it' | 'en'>('rp_language', { sameSite: 'lax', default: () => 'it' })
await setLocale(languageCookie.value === 'en' ? 'en' : 'it')
watch(locale, value => { languageCookie.value = value === 'en' ? 'en' : 'it' })
useHead({ htmlAttrs: { lang: locale }, title: computed(() => `RustPress | ${t('admin')}`) })
const contentLocale = ref<'' | 'it' | 'en'>('')
const navigationLocale = ref<'it' | 'en'>('it')
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
const editorGeneration = ref(0)

function blank(): ContentInput {
  return { content_type: 'page', slug: '', title: '', blocks: [], markdown: '', locale: locale.value === 'en' ? 'en' : 'it', fields: {}, extra_fields: [],
    seo: { description: '', canonical: null, noindex: false }, status: 'draft',
    publish_at: new Date().toISOString(), version: null }
}
function inputOf(c: Content): ContentInput {
  return { content_type: c.content_type, slug: c.slug, title: c.title, blocks: c.blocks, markdown: c.markdown ?? null, locale: c.locale || 'it', fields: c.fields,
    extra_fields: c.extra_fields, seo: c.seo, status: c.status, publish_at: c.publish_at, version: c.version }
}
async function run(operation: () => Promise<void>) {
  if (busy.value) return
  busy.value = true
  failure.value = ''
  notice.value = ''
  try { await operation() } catch (error) { failure.value = errorMessage(error) } finally { busy.value = false }
}
function discardAllowed() { return !dirty.value || window.confirm(t('discard')) }
async function loadList() {
  const data = await $fetch<Page<Content>>('/api/v1/contents', { query: { limit: 20, offset: offset.value, q: search.value, ...(contentLocale.value ? { locale: contentLocale.value } : {}) } })
  contents.value = data.items
}
async function loadResources() {
  const [contentTypes, images, navigation] = await Promise.all([
    $fetch<ContentType[]>('/api/v1/types'),
    $fetch<Page<Media>>('/api/v1/media', { query: { limit: 100 } }),
    $fetch<NavItem[]>('/api/v1/public/navigation/main', { query: { locale: navigationLocale.value } }),
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
  if (!Array.isArray(parsed)) throw new Error(t('arrayRequired'))
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
        || ('required' in field && typeof field.required !== 'boolean')) throw new Error(t('invalidField'))
      const kind = field.kind
      if (kind !== 'text' && kind !== 'number' && kind !== 'boolean' && kind !== 'date' && kind !== 'media' && kind !== 'relation') throw new Error(t('invalidKind'))
      fields.push({ key: field.key, label: field.label, kind, required: 'required' in field && field.required === true })
    }
    draft.value.extra_fields = fields
    dirty.value = true
    notice.value = t('schemaApplied')
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
    notice.value = t('saved', { version: saved.version })
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
  draft.value = { ...inputOf(revision.snapshot), locale: draft.value.locale, status: 'draft', version: currentVersion }
  extraFieldsJson.value = JSON.stringify(draft.value.extra_fields, null, 2)
  editorGeneration.value++
  dirty.value = true
  notice.value = t('restored')
}
async function saveType() {
  await run(async () => {
    await $fetch(`/api/v1/types/${encodeURIComponent(typeSlug.value)}`, { method: 'PUT',
      body: { name: typeName.value, fields: parseArray(typeFields.value) } })
    types.value = await $fetch<ContentType[]>('/api/v1/types')
    notice.value = t('schemaSaved')
  })
}
async function saveNavigation() {
  await run(async () => {
    await $fetch('/api/v1/navigation/main', { method: 'PUT', query: { locale: navigationLocale.value }, body: parseArray(navigationJson.value) })
    notice.value = t('navigationSaved')
  })
}
function selectFile(event: Event) {
  if (event.target instanceof HTMLInputElement) selectedFile.value = event.target.files?.[0] || null
}
async function upload() {
  await run(async () => {
    if (!selectedFile.value) throw new Error(t('selectImage'))
    const form = new FormData()
    form.append('file', selectedFile.value)
    form.append('alt', alt.value)
    const uploaded = await $fetch<Media>('/api/v1/media', { method: 'POST', body: form })
    media.value.unshift(uploaded)
    notice.value = t('imageUploaded', { id: uploaded.id })
  })
}
async function loadNavigation() {
  await run(async () => {
    const navigation = await $fetch<NavItem[]>('/api/v1/public/navigation/main', { query: { locale: navigationLocale.value } })
    navigationJson.value = JSON.stringify(navigation, null, 2)
  })
}
function translate() {
  if (!selectedId.value || !discardAllowed()) return
  const sourceId = selectedId.value
  draft.value = { ...structuredClone(toRaw(draft.value)), locale: draft.value.locale === 'it' ? 'en' : 'it',
    translation_of: sourceId, slug: '', status: 'draft', version: null, seo: { description: '', canonical: null, noindex: false } }
  selectedId.value = null
  revisions.value = []
  extraFieldsJson.value = JSON.stringify(draft.value.extra_fields, null, 2)
  dirty.value = true
  notice.value = t('translationHelp')
}
async function convertLegacy() {
  await run(async () => {
    const escape = (text: string) => text.replace(/[\\`*_{}[\]()#+.!<>~-]/g, '\\$&')
    const parts: string[] = []
    for (const block of draft.value.blocks) {
      if (block.kind === 'paragraph') parts.push(escape(block.text))
      else if (block.kind === 'heading') parts.push(`${'#'.repeat(block.level)} ${escape(block.text)}`)
      else {
        const image = media.value.find(m => m.id === block.media_id) || await $fetch<Media>(`/api/v1/public/media/${block.media_id}`)
        const url = image.variants.at(-1)?.url
        if (!url) throw new Error(t('imageUnavailable', { alt: block.alt }))
        parts.push(`![${escape(block.alt)}](${url})`)
      }
    }
    draft.value.markdown = parts.join('\n\n')
    draft.value.blocks = []
    dirty.value = true
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
  <a class="btn btn-primary absolute -top-20 focus:top-2" href="#main">{{ t('skip') }}</a>
  <header class="navbar bg-base-200 px-6 gap-4">
    <span class="text-xl font-bold flex-1">RustPress <span class="badge">0.1</span></span>
    <span v-if="actor">{{ actor.role }}</span>
    <label class="flex gap-2 items-center">{{ t('language') }}<select :value="locale" :aria-label="t('language')" class="select select-sm" @change="setLocale(($event.target as HTMLSelectElement).value === 'en' ? 'en' : 'it')"><option value="it">Italiano</option><option value="en">English</option></select></label>
    <button v-if="actor" class="btn btn-ghost" :disabled="busy" @click="logout">{{ t('logout') }}</button>
  </header>
  <main id="main" class="max-w-7xl mx-auto p-6">
    <div v-if="failure" role="alert" class="alert alert-error mb-4">{{ failure }}</div>
    <div v-if="notice" role="status" class="alert alert-success mb-4">{{ notice }}</div>
    <p v-if="loading" role="status">{{ t('loading') }}</p>
    <section v-else-if="!actor" class="card bg-base-200 max-w-xl mx-auto my-12">
      <div class="card-body">
        <h1 class="card-title">{{ t('login') }}</h1>
        <p>{{ t('invitedOnly') }}</p>
        <a v-for="provider in providers" :key="provider" class="btn btn-primary mt-2" :href="`/api/v1/auth/${provider}/login`">{{ t('loginWith', { provider }) }}</a>
        <p v-if="!providers.length" class="alert alert-warning">{{ t('noProviders') }}</p>
      </div>
    </section>
    <template v-else>
      <nav :aria-label="t('admin')" class="flex flex-wrap gap-2 mb-6">
        <button v-for="item in ['contents', 'media', ...(actor.role === 'admin' ? ['schemas'] : []), ...(editorial ? ['navigation'] : [])]" :key="item"
          class="btn" :class="{ 'btn-primary': tab === item }" :aria-current="tab === item ? 'page' : undefined" @click="tab = item">
          {{ t(item) }}
        </button>
      </nav>
      <section v-if="tab === 'contents'" class="grid lg:grid-cols-[20rem_1fr] gap-8">
        <aside>
          <h1 class="text-2xl font-bold mb-4">{{ t('contents') }}</h1>
          <button class="btn btn-primary mb-4" :disabled="busy" @click="newContent">{{ t('newContent') }}</button>
          <label class="fieldset">{{ t('contentLanguage') }}<select v-model="contentLocale" :aria-label="t('contentLanguage')" class="select" @change="run(async () => { offset = 0; await loadList() })"><option value="">{{ t('allLanguages') }}</option><option value="it">Italiano</option><option value="en">English</option></select></label>
          <form class="flex gap-2 mb-4" @submit.prevent="run(async () => { offset = 0; await loadList() })">
            <input v-model="search" :aria-label="t('searchContents')" type="search" class="input w-full" maxlength="200">
            <button class="btn" :disabled="busy">{{ t('search') }}</button>
          </form>
          <ul class="menu w-full p-0">
            <li v-for="content in contents" :key="content.id"><button :disabled="busy" @click="edit(content.id)">
              <span>{{ content.title }}</span><span class="badge">{{ content.locale }}</span><span class="badge badge-outline">{{ t(content.status) }}</span>
            </button></li>
          </ul>
          <div class="flex gap-2 mt-4">
            <button class="btn btn-sm" :disabled="busy || offset === 0" @click="run(async () => { offset -= 20; await loadList() })">{{ t('previous') }}</button>
            <button class="btn btn-sm" :disabled="busy || contents.length < 20" @click="run(async () => { offset += 20; await loadList() })">{{ t('next') }}</button>
          </div>
        </aside>
        <form class="space-y-5" @submit.prevent="save" @input="dirty = true" @change="dirty = true">
          <h2 class="text-2xl font-bold">{{ t(selectedId ? 'edit' : 'create') }} <span v-if="dirty" class="badge badge-warning">{{ t('unsaved') }}</span></h2>
          <label class="fieldset">{{ t('contentLanguage') }}<select v-model="draft.locale" :aria-label="t('contentLanguage')" class="select" :disabled="!!selectedId || !!draft.translation_of"><option value="it">Italiano</option><option value="en">English</option></select></label>
          <button v-if="selectedId" type="button" class="btn" :disabled="busy" @click="translate">{{ t('translate', { language: draft.locale === 'it' ? 'English' : 'Italiano' }) }}</button>
          <p v-if="draft.translation_of" class="alert">{{ t('translationSource') }}: {{ draft.translation_of }}</p>
          <div class="grid sm:grid-cols-2 gap-4">
            <label class="fieldset">{{ t('type') }}<select v-model="draft.content_type" :aria-label="t('type')" class="select w-full" :disabled="!!selectedId || !!draft.translation_of"><option v-for="type in types" :key="type.slug" :value="type.slug">{{ type.name }}</option></select></label>
            <label class="fieldset">{{ t('slug') }}<input v-model="draft.slug" class="input w-full" required maxlength="120" pattern="[a-z0-9]+(-[a-z0-9]+)*"></label>
          </div>
          <label class="fieldset">{{ t('title') }}<input v-model="draft.title" class="input w-full" required maxlength="300"></label>
          <section v-if="draft.markdown === null" class="space-y-3">
            <p class="alert alert-info">{{ t('legacyHelp') }}</p>
            <pre class="text-sm whitespace-pre-wrap">{{ JSON.stringify(draft.blocks, null, 2) }}</pre>
            <button type="button" class="btn" :disabled="busy" @click="convertLegacy">{{ t('convert') }}</button>
          </section>
          <ClientOnly v-else><MarkdownEditor :key="`${selectedId}:${draft.locale}:${draft.version}:${editorGeneration}`" v-model="draft.markdown" :media="media" @update:model-value="dirty = true" /><template #fallback><p>{{ t('editorLoading') }}</p></template></ClientOnly>
          <fieldset v-if="schema.length" class="grid sm:grid-cols-2 gap-4">
            <legend class="font-semibold mb-3">{{ t('customFields') }}</legend>
            <label v-for="field in schema" :key="field.key" class="fieldset">{{ field.label }} {{ field.required ? '*' : '' }}
              <input v-if="field.kind === 'boolean'" v-model="draft.fields[field.key]" type="checkbox" class="checkbox" :required="false">
              <input v-else-if="field.kind === 'number'" v-model.number="draft.fields[field.key]" type="number" step="any" class="input w-full" :required="field.required">
              <input v-else v-model="draft.fields[field.key]" :type="field.kind === 'date' ? 'date' : 'text'" class="input w-full" :required="field.required" :placeholder="['media','relation'].includes(field.kind) ? 'UUID' : ''">
            </label>
          </fieldset>
          <details v-if="actor.role === 'admin'" class="collapse collapse-arrow border border-base-300">
            <summary class="collapse-title">{{ t('extraSchema') }}</summary>
            <div class="collapse-content"><label class="fieldset">{{ t('extraJson') }}<textarea v-model="extraFieldsJson" class="textarea w-full font-mono" rows="6"></textarea></label><button type="button" class="btn btn-sm mt-2" :disabled="busy" @click="applyExtraFields">{{ t('applySchema') }}</button><p>{{ t('applyHelp') }}</p></div>
          </details>
          <fieldset class="space-y-3">
            <legend class="font-semibold">SEO e GEO</legend>
            <label class="fieldset">{{ t('description') }}<textarea v-model="draft.seo.description" class="textarea w-full" maxlength="500"></textarea></label>
            <label class="fieldset">{{ t('canonical') }}<input :value="draft.seo.canonical || ''" class="input w-full" @input="draft.seo.canonical = ($event.target as HTMLInputElement).value || null"></label>
            <label class="flex gap-2"><input v-model="draft.seo.noindex" type="checkbox" class="checkbox">{{ t('noindex') }}</label>
          </fieldset>
          <label class="fieldset">{{ t('publication') }}<input v-model="draft.publish_at" class="input w-full" required></label>
          <label class="fieldset">{{ t('status') }}<select v-model="draft.status" :aria-label="t('status')" class="select" :disabled="!editorial"><option value="draft">{{ t('draft') }}</option><option v-if="editorial" value="published">{{ t('published') }}</option><option v-if="editorial" value="archived">{{ t('archived') }}</option></select></label>
          <div class="flex gap-3">
            <button class="btn btn-primary" :disabled="busy">{{ t('save') }}</button>
            <button v-if="selectedId" type="button" class="btn" :disabled="busy" @click="showRevisions">{{ t('revisions') }}</button>
          </div>
          <ul v-if="revisions.length" class="menu border border-base-300 rounded-box">
            <li v-for="revision in revisions" :key="revision.id"><button type="button" @click="restore(revision)">{{ t('loadRevision', { version: revision.version, date: revision.created_at }) }}</button></li>
          </ul>
        </form>
      </section>
      <section v-if="tab === 'media'" class="space-y-6">
        <h1 class="text-2xl font-bold">{{ t('library') }}</h1>
        <p>{{ t('mediaHelp') }}</p>
        <form class="flex flex-wrap items-end gap-4" @submit.prevent="upload">
          <label class="fieldset">{{ t('image') }}<input type="file" accept="image/jpeg,image/png,image/webp" class="file-input" required @change="selectFile"></label>
          <label class="fieldset">{{ t('alt') }}<input v-model="alt" class="input" required maxlength="500"></label>
          <button class="btn btn-primary" :disabled="busy">{{ t('upload') }}</button>
        </form>
        <div class="grid sm:grid-cols-3 gap-4">
          <figure v-for="img in media" :key="img.id" class="card border border-base-300 p-4">
            <img :src="img.variants[0]?.url" :alt="img.alt" loading="lazy" class="h-40 object-contain">
            <figcaption class="mt-2 break-all">{{ img.alt }}<br><code class="text-xs">{{ img.id }}</code></figcaption>
          </figure>
        </div>
        <p>{{ t('mediaLimit') }}</p>
      </section>
      <section v-if="tab === 'schemas' && actor.role === 'admin'" class="max-w-3xl space-y-4">
        <h1 class="text-2xl font-bold">{{ t('schemaTitle') }}</h1>
        <p>{{ t('schemaHelp') }}</p>
        <label class="fieldset">{{ t('existingType') }}<select v-model="typeSlug" class="select" @change="loadType"><option v-for="type in types" :key="type.slug" :value="type.slug">{{ type.name }}</option></select></label>
        <form class="space-y-4" @submit.prevent="saveType">
          <label class="fieldset">{{ t('typeSlug') }}<input v-model="typeSlug" class="input" required></label>
          <label class="fieldset">{{ t('name') }}<input v-model="typeName" class="input" required></label>
          <label class="fieldset">{{ t('fieldJson') }}<textarea v-model="typeFields" class="textarea w-full font-mono" rows="12" spellcheck="false"></textarea></label>
          <pre class="mockup-code p-4 whitespace-pre-wrap">[{"key":"subtitle","label":"Sottotitolo","kind":"text","required":false}]</pre>
          <button class="btn btn-primary" :disabled="busy">{{ t('saveSchema') }}</button>
        </form>
      </section>
      <section v-if="tab === 'navigation' && editorial" class="max-w-3xl">
        <h1 class="text-2xl font-bold mb-4">{{ t('mainNavigation') }}</h1>
        <label class="fieldset">{{ t('contentLanguage') }}<select v-model="navigationLocale" class="select" @change="loadNavigation"><option value="it">Italiano</option><option value="en">English</option></select></label>
        <form class="space-y-4" @submit.prevent="saveNavigation">
          <label class="fieldset">{{ t('navigationJson') }}<textarea v-model="navigationJson" class="textarea w-full font-mono" rows="12"></textarea></label>
          <pre class="mockup-code p-4 whitespace-pre-wrap">[{"label":"Home","href":"/","children":[]}]</pre>
          <button class="btn btn-primary" :disabled="busy">{{ t('saveNavigation') }}</button>
        </form>
      </section>
    </template>
  </main>
</template>
