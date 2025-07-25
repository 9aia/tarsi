import process from 'node:process'
import { GoogleGenAI } from '@google/genai'

let ai: GoogleGenAI | null = null

export async function generateContent({
  prompt,
  model,
}: {
  prompt: string
  model: string
}) {
  if (!process.env.TARSI_GEMINI_API_KEY) {
    throw new Error('TARSI_GEMINI_API_KEY is not set')
  }

  if (!ai) {
    ai = new GoogleGenAI({ apiKey: process.env.TARSI_GEMINI_API_KEY! })
  }

  const response = await ai.models.generateContent({
    model,
    contents: prompt,
  })

  return response.text
}
