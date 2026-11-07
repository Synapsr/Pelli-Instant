/**
 * Third-party models and software shipped with Pelli Instant (About page).
 * Keep the model entries in sync with `models/manifest.json`.
 */
export interface Licence {
  name: string
  author: string
  licence: string
  url: string
  kind: 'model' | 'software' | 'font' | 'photos' | 'voice'
}

export const LICENCES: Licence[] = [
  { kind: 'model', name: 'YuNet face detector', author: 'OpenCV Zoo', licence: 'MIT', url: 'https://github.com/opencv/opencv_zoo' },
  { kind: 'model', name: 'MediaPipe Face Mesh V2 (eye state)', author: 'Google, ONNX conversion by PINTO0309', licence: 'Apache-2.0', url: 'https://github.com/google-ai-edge/mediapipe' },
  { kind: 'model', name: 'MediaPipe Face Blendshapes V2 (eyes and expression)', author: 'Google, ONNX conversion by Pelli', licence: 'Apache-2.0', url: 'https://github.com/google-ai-edge/mediapipe' },
  { kind: 'model', name: 'CLIP ViT-B/32', author: 'OpenAI, ONNX by Xenova', licence: 'MIT', url: 'https://github.com/openai/CLIP' },
  { kind: 'model', name: 'LAION Aesthetic Predictor', author: 'LAION', licence: 'MIT', url: 'https://github.com/LAION-AI/aesthetic-predictor' },
  // Maximum precision pack (downloaded on request).
  { kind: 'model', name: 'CLIP ViT-L/14 (Maximum precision)', author: 'OpenAI, ONNX by Xenova', licence: 'MIT', url: 'https://github.com/openai/CLIP' },
  { kind: 'model', name: 'LAION-Aesthetics Predictor V2 (Maximum precision)', author: 'Christoph Schuhmann (LAION)', licence: 'Apache-2.0', url: 'https://github.com/christophschuhmann/improved-aesthetic-predictor' },
  { kind: 'model', name: 'DINOv2 ViT-B/14 (Maximum precision)', author: 'Meta AI, ONNX by onnx-community', licence: 'Apache-2.0', url: 'https://github.com/facebookresearch/dinov2' },
  { kind: 'model', name: 'RF-DETR-Seg Small (Maximum precision)', author: 'Roboflow, ONNX conversion by Pelli', licence: 'Apache-2.0', url: 'https://github.com/roboflow/rf-detr' },
  { kind: 'model', name: 'RetinaFace ResNet-50 (Maximum precision)', author: 'biubug6, ONNX conversion by Pelli', licence: 'MIT', url: 'https://github.com/biubug6/Pytorch_Retinaface' },
  { kind: 'software', name: 'ONNX Runtime', author: 'Microsoft', licence: 'MIT', url: 'https://github.com/microsoft/onnxruntime' },
  { kind: 'software', name: 'ort', author: 'pyke.io', licence: 'MIT / Apache-2.0', url: 'https://ort.pyke.io' },
  { kind: 'software', name: 'jpeg-encoder', author: 'Volker Ströbel. This software is based in part on the work of the Independent JPEG Group', licence: 'MIT / Apache-2.0, IJG', url: 'https://github.com/vstroebel/jpeg-encoder' },
  { kind: 'software', name: 'zune-jpeg, jpeg-decoder, fast_image_resize, image', author: 'Rust image community', licence: 'MIT / Apache-2.0 / Zlib', url: 'https://github.com/image-rs' },
  { kind: 'software', name: 'kamadak-exif', author: 'KAMADA Ken\'ichi', licence: 'BSD-2-Clause', url: 'https://github.com/kamadak/exif-rs' },
  { kind: 'software', name: 'instant-clip-tokenizer', author: 'Instant Domain Search', licence: 'MIT', url: 'https://github.com/instant-labs/instant-clip-tokenizer' },
  { kind: 'software', name: 'SQLite (rusqlite)', author: 'SQLite authors', licence: 'Public domain / MIT', url: 'https://www.sqlite.org' },
  { kind: 'software', name: 'Tauri', author: 'Tauri Programme', licence: 'MIT / Apache-2.0', url: 'https://tauri.app' },
  { kind: 'software', name: 'React', author: 'Meta', licence: 'MIT', url: 'https://react.dev' },
  { kind: 'software', name: 'HeroUI', author: 'HeroUI', licence: 'MIT', url: 'https://heroui.com' },
  { kind: 'software', name: 'Tailwind CSS', author: 'Tailwind Labs', licence: 'MIT', url: 'https://tailwindcss.com' },
  { kind: 'software', name: 'TanStack Virtual', author: 'TanStack', licence: 'MIT', url: 'https://tanstack.com/virtual' },
  { kind: 'software', name: 'Zustand', author: 'Poimandres', licence: 'MIT', url: 'https://github.com/pmndrs/zustand' },
  { kind: 'software', name: 'Framer Motion', author: 'Motion Division', licence: 'MIT', url: 'https://motion.dev' },
  { kind: 'software', name: 'Lucide', author: 'Lucide contributors', licence: 'ISC', url: 'https://lucide.dev' },
  { kind: 'font', name: 'Cabinet Grotesk', author: 'Indian Type Foundry', licence: 'ITF Free Font License', url: 'https://www.fontshare.com/fonts/cabinet-grotesk' },
  {
    kind: 'photos',
    name: 'Welcome tour photos',
    author: 'Henry Söderlund, Tg-Pint, exousiavampira, Tobyotter, WriteAwaybyKarylle, firemedic58, apdk, pixellaphoto, d26b73, jarrodlombardo (Flickr)',
    licence: 'CC BY 2.0 / CC0',
    url: 'https://github.com/Synapsr/Pelli-Instant/blob/main/src/assets/onboarding/CREDITS.md',
  },
  {
    kind: 'voice',
    name: 'Welcome tour voice-over',
    author: 'ElevenLabs (Eleven v4): Hope, Victoria, Cristina, Carla Blum, Tiziana, Carla, Ruth, Marta, Malin',
    licence: 'ElevenLabs terms (commercial use)',
    url: 'https://github.com/Synapsr/Pelli-Instant/blob/main/src/assets/onboarding/CREDITS.md',
  },
]
