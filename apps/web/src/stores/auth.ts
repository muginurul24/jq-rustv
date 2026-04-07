import { computed, ref } from 'vue'
import { defineStore } from 'pinia'
import { isAxiosError } from 'axios'
import api from '@/lib/axios'

export interface AuthUser {
  id: number
  username: string
  name: string
  role: string
}

export interface LoginPayload {
  username: string
  password: string
  captcha_id: string
  captcha_answer: string
}

interface ApiErrorResponse {
  message?: string
}

interface LoginResponse {
  success: boolean
  user: AuthUser
}

interface MeResponse {
  success: boolean
  user: AuthUser
}

interface LogoutResponse {
  success: boolean
}

function isUnauthorizedError(error: unknown) {
  return isAxiosError(error) && error.response?.status === 401
}

function extractErrorMessage(error: unknown, fallback: string) {
  if (!isAxiosError<ApiErrorResponse>(error)) {
    return fallback
  }

  return error.response?.data?.message ?? fallback
}

export const useAuthStore = defineStore('auth', () => {
  const user = ref<AuthUser | null>(null)
  const isLoading = ref(false)
  const hasFetchedUser = ref(false)
  const lastError = ref<string | null>(null)
  const isAuthenticated = computed(() => user.value !== null)

  function clearError() {
    lastError.value = null
  }

  async function fetchUser() {
    isLoading.value = true
    clearError()

    try {
      const { data } = await api.get<MeResponse>('/auth/me')
      user.value = data.user
      hasFetchedUser.value = true
      return data.user
    } catch (error) {
      hasFetchedUser.value = true

      if (isUnauthorizedError(error)) {
        user.value = null
        return null
      }

      lastError.value = extractErrorMessage(error, 'Failed to load current user')
      throw error
    } finally {
      isLoading.value = false
    }
  }

  async function login(payload: LoginPayload) {
    isLoading.value = true
    clearError()

    try {
      const { data } = await api.post<LoginResponse>('/auth/login', payload)
      user.value = data.user
      hasFetchedUser.value = true
      return data.user
    } catch (error) {
      lastError.value = extractErrorMessage(error, 'Login failed')
      throw error
    } finally {
      isLoading.value = false
    }
  }

  async function logout() {
    isLoading.value = true
    clearError()

    try {
      await api.post<LogoutResponse>('/auth/logout')
    } catch (error) {
      if (!isUnauthorizedError(error)) {
        lastError.value = extractErrorMessage(error, 'Logout failed')
        throw error
      }
    } finally {
      user.value = null
      hasFetchedUser.value = true
      isLoading.value = false
    }
  }

  return {
    user,
    isLoading,
    hasFetchedUser,
    lastError,
    isAuthenticated,
    clearError,
    fetchUser,
    login,
    logout,
  }
})
