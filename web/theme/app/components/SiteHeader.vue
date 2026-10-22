<script setup lang="ts">
import type { NavItem } from '../../../shared/types'
const { data: navigation, error } = await useFetch<NavItem[]>('/api/v1/public/navigation/main')
</script>

<template>
  <a class="btn btn-primary absolute -top-20 focus:top-2 z-50" href="#main">Vai al contenuto</a>
  <header class="border-b border-base-300">
    <div class="navbar mx-auto max-w-5xl flex-wrap gap-4">
      <NuxtLink to="/" class="text-xl font-bold flex-1">RustPress</NuxtLink>
      <nav aria-label="Navigazione principale">
        <ul class="menu menu-horizontal">
          <li v-for="item in navigation" :key="item.href">
            <a :href="item.href">{{ item.label }}</a>
            <ul v-if="item.children.length">
              <li v-for="child in item.children" :key="child.href">
                <a :href="child.href">{{ child.label }}</a>
                <ul v-if="child.children.length">
                  <li v-for="leaf in child.children" :key="leaf.href"><a :href="leaf.href">{{ leaf.label }}</a></li>
                </ul>
              </li>
            </ul>
          </li>
        </ul>
      </nav>
    </div>
    <p v-if="error" class="mx-auto max-w-5xl text-error" role="alert">Navigazione non disponibile.</p>
  </header>
</template>
