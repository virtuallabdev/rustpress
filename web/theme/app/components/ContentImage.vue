<script setup lang="ts">
import type { Media } from '../../../shared/types'
const props = defineProps<{ id: string; alt: string }>()
const { data, error } = await useFetch<Media>(`/api/v1/public/media/${props.id}`)
const source = computed(() => data.value?.variants.at(-1)?.url)
const srcset = computed(() => data.value?.variants.map(v => `${v.url} ${v.width}w`).join(', '))
</script>

<template>
  <figure class="my-6">
    <img v-if="data && source" :src="source" :srcset="srcset" sizes="(max-width: 768px) 100vw, 768px"
      :width="data.width" :height="data.height" :alt="alt" loading="lazy" decoding="async" class="rounded-xl max-w-full h-auto">
    <p v-if="error" class="text-error" role="alert">Immagine non disponibile: {{ alt }}</p>
  </figure>
</template>
