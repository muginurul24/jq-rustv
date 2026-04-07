<script setup lang="ts">
import { defineAsyncComponent } from 'vue'
import AppSidebar from '@/components/AppSidebar.vue'
import SectionCards from '@/components/SectionCards.vue'
import SiteHeader from '@/components/SiteHeader.vue'
import { Skeleton } from '@/components/ui/skeleton'
import { SidebarInset, SidebarProvider } from '@/components/ui/sidebar'

const ChartAreaInteractive = defineAsyncComponent(
  () => import('@/components/ChartAreaInteractive.vue'),
)
const DataTable = defineAsyncComponent(() => import('@/components/DataTable.vue'))

interface DashboardRow {
  id: number
  header: string
  type: string
  status: string
  target: string
  limit: string
  reviewer: string
}

defineProps<{
  rows: DashboardRow[]
}>()
</script>

<template>
  <SidebarProvider
    :style="{
      '--header-height': '4rem',
      '--sidebar-width': '18rem',
    }"
  >
    <AppSidebar />
    <SidebarInset>
      <SiteHeader />
      <div class="@container/main flex flex-1 flex-col">
        <div class="flex flex-1 flex-col gap-4 py-4 md:gap-6 md:py-6">
          <SectionCards />
          <div class="px-4 lg:px-6">
            <Suspense>
              <ChartAreaInteractive />
              <template #fallback>
                <div class="rounded-xl border p-6">
                  <Skeleton class="h-6 w-40" />
                  <Skeleton class="mt-4 h-[250px] w-full" />
                </div>
              </template>
            </Suspense>
          </div>
          <Suspense>
            <DataTable :data="rows" />
            <template #fallback>
              <div class="space-y-4 px-4 lg:px-6">
                <Skeleton class="h-10 w-full" />
                <Skeleton class="h-64 w-full" />
              </div>
            </template>
          </Suspense>
        </div>
      </div>
    </SidebarInset>
  </SidebarProvider>
</template>
