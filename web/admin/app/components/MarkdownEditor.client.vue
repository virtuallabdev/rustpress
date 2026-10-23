<script setup lang="ts">
import { EditorContent, useEditor } from '@tiptap/vue-3'
import StarterKit from '@tiptap/starter-kit'
import { Markdown } from '@tiptap/markdown'
import Image from '@tiptap/extension-image'
import { TableKit } from '@tiptap/extension-table'
import type { Media } from '../../../shared/types'
import { errorMessage } from '../../../shared/types'

const model = defineModel<string>({ required: true })
const props = defineProps<{ media: Media[] }>()
const { t } = useI18n()
const mode = ref<'source' | 'visual' | 'preview'>('source')
const html = ref('')
const failure = ref('')
const busy = ref(false)
const link = ref('')
const image = ref('')
function safeLink(value: string) {
  return /^https:\/\/[^\s]+$/i.test(value) || /^\/(?!\/)[^\\\s]*$/.test(value) || /^#[^\s]*$/.test(value)
}
const SafeImage = Image.extend({
  addInputRules() { return [] },
  parseHTML() {
    return [{ tag: 'img[src]', getAttrs: element =>
      /^\/media\/[0-9a-f-]{36}\/\d{1,4}\.webp$/.test(element.getAttribute('src') || '') ? null : false }]
  },
})
const editor = useEditor({
  extensions: [
    StarterKit.configure({ heading: { levels: [1,2,3,4,5,6] }, underline: false,
      link: { openOnClick: false, isAllowedUri: safeLink } }),
    Markdown,
    SafeImage,
    TableKit,
  ],
  content: '',
  editorProps: { attributes: { class: 'markdown-body min-h-64 border border-base-300 rounded-lg p-4', 'aria-label': t('markdown') } },
  onUpdate: ({ editor }) => { model.value = editor.getMarkdown() },
})
async function changeMode(target: typeof mode.value) {
  if (target === 'source') { mode.value = target; return }
  busy.value = true
  failure.value = ''
  try {
    const result = await $fetch<{ body_html: string }>('/api/v1/markdown/preview', { method: 'POST', body: { markdown: model.value } })
    html.value = result.body_html
    if (target === 'visual') editor.value?.commands.setContent(result.body_html, { emitUpdate: false })
    mode.value = target
  } catch (error) { failure.value = errorMessage(error) } finally { busy.value = false }
}
function insertImage() {
  const media = props.media.find(m => m.id === image.value)
  const variant = media?.variants.at(-1)
  if (media && variant) editor.value?.chain().focus().setImage({ src: variant.url, alt: media.alt }).run()
}
onBeforeUnmount(() => editor.value?.destroy())
</script>

<template>
  <section class="space-y-3" :aria-label="t('markdown')">
    <div class="flex flex-wrap gap-2">
      <button v-for="target in (['source','visual','preview'] as const)" :key="target" type="button"
        class="btn btn-sm" :class="{ 'btn-primary': mode === target }" :aria-pressed="mode === target" :disabled="busy"
        @click="changeMode(target)">{{ t(target) }}</button>
    </div>
    <p class="text-sm opacity-80">{{ t('previewHelp') }}</p>
    <p v-if="failure" role="alert" class="alert alert-error">{{ failure }}</p>
    <textarea v-if="mode === 'source'" v-model="model" :aria-label="t('markdown')" class="textarea w-full font-mono min-h-80" spellcheck="false" />
    <template v-if="mode === 'visual' && editor">
      <div class="flex flex-wrap gap-2" role="toolbar" :aria-label="t('visual')">
        <button type="button" class="btn btn-sm" @click="editor.chain().focus().toggleBold().run()">{{ t('bold') }}</button>
        <button type="button" class="btn btn-sm" @click="editor.chain().focus().toggleItalic().run()">{{ t('italic') }}</button>
        <button type="button" class="btn btn-sm" @click="editor.chain().focus().toggleHeading({ level: 2 }).run()">H2</button>
        <button type="button" class="btn btn-sm" @click="editor.chain().focus().toggleHeading({ level: 3 }).run()">H3</button>
        <button type="button" class="btn btn-sm" @click="editor.chain().focus().toggleBulletList().run()">{{ t('bulletList') }}</button>
        <button type="button" class="btn btn-sm" @click="editor.chain().focus().toggleOrderedList().run()">{{ t('orderedList') }}</button>
        <button type="button" class="btn btn-sm" @click="editor.chain().focus().toggleBlockquote().run()">{{ t('quote') }}</button>
        <button type="button" class="btn btn-sm" @click="editor.chain().focus().toggleCodeBlock().run()">{{ t('code') }}</button>
        <button type="button" class="btn btn-sm" @click="editor.chain().focus().undo().run()">{{ t('undo') }}</button>
        <button type="button" class="btn btn-sm" @click="editor.chain().focus().redo().run()">{{ t('redo') }}</button>
      </div>
      <div class="flex gap-2">
        <input v-model="link" class="input flex-1" :aria-label="t('linkUrl')" :placeholder="t('linkUrl')">
        <button type="button" class="btn" :disabled="!safeLink(link)" @click="editor.chain().focus().setLink({ href: link }).run()">{{ t('addLink') }}</button>
      </div>
      <div class="flex gap-2">
        <select v-model="image" class="select flex-1" :aria-label="t('image')"><option value="">{{ t('select') }}</option><option v-for="item in media" :key="item.id" :value="item.id">{{ item.alt }}</option></select>
        <button type="button" class="btn" :disabled="!image" @click="insertImage">{{ t('insertImage') }}</button>
      </div>
      <EditorContent :editor="editor" />
    </template>
    <!-- HTML is exclusively produced by the authenticated Rust sanitizer endpoint. -->
    <div v-if="mode === 'preview'" class="markdown-body border border-base-300 p-4 rounded-lg" v-html="html" />
  </section>
</template>
