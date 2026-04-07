<script setup lang="ts">
import type { HTMLAttributes } from 'vue'
import { computed, onMounted, reactive, ref } from 'vue'
import { isAxiosError } from 'axios'
import { useRouter } from 'vue-router'
import { cn } from '@/lib/utils'
import api from '@/lib/axios'
import { useAuthStore } from '@/stores/auth'
import { Button } from '@/components/ui/button'
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/components/ui/card'
import {
  Field,
  FieldDescription,
  FieldError,
  FieldGroup,
  FieldLabel,
} from '@/components/ui/field'
import { Input } from '@/components/ui/input'

interface CaptchaResponse {
  captcha_id: string
  image: string
}

interface ApiErrorResponse {
  message?: string
}

const props = defineProps<{
  class?: HTMLAttributes['class']
}>()

const router = useRouter()
const auth = useAuthStore()

const form = reactive({
  username: '',
  password: '',
  captcha_answer: '',
})

const captchaId = ref('')
const captchaMarkup = ref('')
const captchaLoadError = ref<string | null>(null)
const isCaptchaLoading = ref(false)

const captchaImageUrl = computed(() => {
  if (!captchaMarkup.value) {
    return ''
  }

  return `data:image/svg+xml;charset=utf-8,${encodeURIComponent(captchaMarkup.value)}`
})

const formErrors = computed(() =>
  [captchaLoadError.value, auth.lastError].filter(
    (error): error is string => typeof error === 'string' && error.length > 0,
  ),
)

async function loadCaptcha() {
  isCaptchaLoading.value = true
  captchaLoadError.value = null

  try {
    const { data } = await api.get<CaptchaResponse>('/auth/captcha')
    captchaId.value = data.captcha_id
    captchaMarkup.value = data.image
    form.captcha_answer = ''
  } catch (error) {
    captchaId.value = ''
    captchaMarkup.value = ''
    captchaLoadError.value = isAxiosError<ApiErrorResponse>(error)
      ? error.response?.data?.message ?? 'Failed to load captcha'
      : 'Failed to load captcha'
  } finally {
    isCaptchaLoading.value = false
  }
}

async function submit() {
  if (!captchaId.value) {
    await loadCaptcha()
    return
  }

  try {
    await auth.login({
      username: form.username,
      password: form.password,
      captcha_id: captchaId.value,
      captcha_answer: form.captcha_answer,
    })

    await router.replace({ name: 'dashboard' })
  } catch {
    await loadCaptcha()
  }
}

onMounted(async () => {
  await loadCaptcha()
})
</script>

<template>
  <div :class="cn('flex flex-col gap-6', props.class)">
    <Card>
      <CardHeader>
        <CardTitle>Login dashboard</CardTitle>
        <CardDescription>
          Gunakan username, password, dan captcha untuk masuk ke backoffice.
        </CardDescription>
      </CardHeader>
      <CardContent>
        <form class="flex flex-col gap-6" @submit.prevent="submit">
          <FieldGroup>
            <Field>
              <FieldLabel for="username">
                Username
              </FieldLabel>
              <Input
                id="username"
                v-model="form.username"
                autocomplete="username"
                placeholder="justqiuv2"
                required
              />
            </Field>

            <Field>
              <FieldLabel for="password">
                Password
              </FieldLabel>
              <Input
                id="password"
                v-model="form.password"
                type="password"
                autocomplete="current-password"
                required
              />
            </Field>

            <Field>
              <div class="flex items-center justify-between gap-3">
                <FieldLabel for="captcha_answer">
                  Captcha
                </FieldLabel>
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  :disabled="isCaptchaLoading"
                  @click="loadCaptcha"
                >
                  Refresh
                </Button>
              </div>

              <div class="overflow-hidden rounded-lg border bg-muted/30 p-3">
                <div
                  v-if="isCaptchaLoading"
                  class="flex h-[74px] items-center justify-center text-sm text-muted-foreground"
                >
                  Loading captcha...
                </div>
                <img
                  v-else-if="captchaImageUrl"
                  :src="captchaImageUrl"
                  alt="Captcha challenge"
                  class="h-[74px] w-full rounded-md object-contain"
                >
                <div
                  v-else
                  class="flex h-[74px] items-center justify-center text-sm text-muted-foreground"
                >
                  Captcha unavailable
                </div>
              </div>

              <Input
                id="captcha_answer"
                v-model="form.captcha_answer"
                autocomplete="off"
                autocapitalize="characters"
                placeholder="Masukkan captcha"
                required
              />
              <FieldDescription>
                Captcha akan diperbarui setiap kali submit.
              </FieldDescription>
              <FieldError
                v-if="formErrors.length > 0"
                :errors="formErrors"
              />
            </Field>

            <Field>
              <Button
                type="submit"
                class="w-full"
                :disabled="auth.isLoading || isCaptchaLoading"
              >
                {{ auth.isLoading ? 'Signing in...' : 'Login' }}
              </Button>
            </Field>
          </FieldGroup>
        </form>
      </CardContent>
    </Card>
  </div>
</template>
