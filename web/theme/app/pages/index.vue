<script setup lang="ts">
import type { Page, PublicContent } from '../../../shared/types'
const route = useRoute()
const q = computed(() => typeof route.query.q === 'string' ? route.query.q : '')
const offset = computed(() => Math.max(0, Number(route.query.offset) || 0))
const { data, error } = await useFetch<Page<PublicContent>>('/api/v1/public/contents', { query: { q, offset, limit: 12 } })
useSeoMeta({ title: 'RustPress', description: 'Contenuti pubblicati con RustPress' })
</script>

<template>
  <SiteHeader />
  <main id="main" class="max-w-5xl mx-auto p-6">
    <h1 class="text-4xl font-bold my-8">Articoli e pagine</h1>
    <form action="/" method="get" role="search" class="flex gap-2 mb-8">
      <label class="input flex-1">Cerca <input name="q" type="search" :value="q" maxlength="200"></label>
      <button class="btn btn-primary">Cerca</button>
    </form>
    <div v-if="error" role="alert" class="alert alert-error">Impossibile caricare i contenuti. Riprova piu tardi.</div>
    <p v-else-if="!data?.items.length">Nessun contenuto pubblicato.</p>
    <div v-else class="grid md:grid-cols-2 gap-6">
      <article v-for="item in data.items" :key="item.id" class="card border border-base-300">
        <div class="card-body">
          <span class="badge badge-outline">{{ item.content_type }}</span>
          <h2 class="card-title"><NuxtLink :to="`/${item.slug}`">{{ item.title }}</NuxtLink></h2>
          <p>{{ item.seo.description }}</p>
          <time :datetime="item.publish_at">{{ new Date(item.publish_at).toLocaleDateString('it-IT', { timeZone: 'UTC' }) }}</time>
        </div>
      </article>
    </div>
    <nav aria-label="Paginazione" class="flex gap-4 mt-8">
      <NuxtLink v-if="offset > 0" class="btn" :to="{ query: { q, offset: Math.max(0, offset - 12) } }">Precedenti</NuxtLink>
      <NuxtLink v-if="data?.items.length === 12" class="btn" :to="{ query: { q, offset: offset + 12 } }">Successivi</NuxtLink>
    </nav>
  </main>
</template>
