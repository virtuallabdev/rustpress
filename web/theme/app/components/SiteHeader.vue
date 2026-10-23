<script setup lang="ts">
import type { NavItem } from '../../../shared/types'
const { t, locale } = useI18n()
const localePath = useLocalePath()
const { data: navigation, error } = await useFetch<NavItem[]>('/api/v1/public/navigation/main', { query: { locale } })
</script>

<template>
  <a class="btn btn-primary absolute -top-20 focus:top-2 z-50" href="#main">{{ t('skip') }}</a>
  <header class="border-b border-base-300">
    <div class="navbar mx-auto max-w-5xl flex-wrap gap-4">
      <NuxtLink :to="localePath('/')" class="text-xl font-bold flex-1">RustPress</NuxtLink>
      <nav :aria-label="t('language')" class="flex gap-3"><NuxtLink :to="localePath('/', 'it')" lang="it">Italiano</NuxtLink><NuxtLink :to="localePath('/', 'en')" lang="en">English</NuxtLink></nav>
      <nav :aria-label="t('mainNavigation')">
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
    <p v-if="error" class="mx-auto max-w-5xl text-error" role="alert">{{ t('navigationUnavailable') }}</p>
  </header>
</template>
