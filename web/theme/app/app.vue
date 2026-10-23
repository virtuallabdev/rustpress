<script setup lang="ts">
const { locale } = useI18n()
useHead({ htmlAttrs: { lang: locale } })
</script>

<template>
  <NuxtRouteAnnouncer />
  <NuxtPage :key="$route.path" />
</template>
