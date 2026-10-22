<script setup lang="ts">
import type { PublicContent } from '../../../shared/types'
const route = useRoute()
const { data, error } = await useFetch<PublicContent>(`/api/v1/public/contents/${encodeURIComponent(String(route.params.slug))}`)
if (error.value || !data.value) {
  throw createError({ statusCode: error.value?.statusCode === 404 ? 404 : 503, statusMessage: error.value?.statusCode === 404 ? 'Contenuto non trovato' : 'Servizio non disponibile' })
}
const content = data.value
const config = useRuntimeConfig()
const canonical = new URL(content.seo.canonical || `/${content.slug}`, config.public.siteOrigin).href
useSeoMeta({
  title: content.title, description: content.seo.description,
  ogTitle: content.title, ogDescription: content.seo.description,
  ogUrl: canonical, ogType: content.content_type === 'post' ? 'article' : 'website',
  robots: content.seo.noindex ? 'noindex,follow' : 'index,follow',
})
useHead({
  link: [{ rel: 'canonical', href: canonical }],
  script: [{
    type: 'application/ld+json',
    textContent: JSON.stringify({
      '@context': 'https://schema.org', '@type': content.content_type === 'post' ? 'Article' : 'WebPage',
      headline: content.title, description: content.seo.description, url: canonical,
      datePublished: content.publish_at, dateModified: content.updated_at,
    }).replace(/</g, '\\u003c'),
  }],
})
</script>

<template>
  <SiteHeader />
  <main id="main" class="max-w-3xl mx-auto p-6">
    <article>
      <h1 class="text-4xl font-bold my-8">{{ content.title }}</h1>
      <p v-if="content.seo.description" class="text-xl opacity-80 mb-8">{{ content.seo.description }}</p>
      <template v-for="(block, index) in content.blocks" :key="index">
        <p v-if="block.kind === 'paragraph'" class="my-4 leading-8 whitespace-pre-wrap">{{ block.text }}</p>
        <component :is="`h${block.level}`" v-else-if="block.kind === 'heading'" class="text-2xl font-bold mt-8 mb-4">{{ block.text }}</component>
        <ContentImage v-else-if="block.kind === 'image'" :id="block.media_id" :alt="block.alt" />
      </template>
      <dl v-if="Object.keys(content.fields).length" class="grid grid-cols-2 gap-3 border-t border-base-300 mt-10 pt-4">
        <template v-for="(value, key) in content.fields" :key="key"><dt class="font-semibold">{{ key }}</dt><dd>{{ value }}</dd></template>
      </dl>
    </article>
  </main>
</template>
