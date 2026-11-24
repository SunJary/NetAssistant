---
title: Скачать NetAssistant — бесплатно для Windows, Linux и macOS
titleTemplate: ':title'
description: "Скачайте NetAssistant для Windows, Linux и macOS на x64 и ARM64: winget, пакеты deb/AppImage/tar.gz или сборка из исходников, с требованиями к системе."
head:
  - - script
    - type: application/ld+json
    - |
      {
        "@context": "https://schema.org",
        "@graph": [
          {
            "@type": "SoftwareApplication",
            "name": "NetAssistant",
            "applicationCategory": "DeveloperApplication",
            "operatingSystem": "Windows 10+, macOS 10.15+, Linux",
            "description": "Кроссплатформенный инструмент отладки сети с TCP/UDP-клиентом и сервером, MQTT 3.1.1 / 5.0, декодерами кадров, HEX-данными и нагрузочными тестами TCP/UDP.",
            "url": "https://netassistant.trydo.top/ru/download",
            "downloadUrl": "https://github.com/SunJary/NetAssistant/releases",
            "license": "https://www.apache.org/licenses/LICENSE-2.0",
            "offers": { "@type": "Offer", "price": "0", "priceCurrency": "USD" }
          },
          {
            "@type": "FAQPage",
            "mainEntity": [
              {
                "@type": "Question",
                "name": "NetAssistant бесплатен?",
                "acceptedAnswer": {
                  "@type": "Answer",
                  "text": "Да. NetAssistant — открытый проект под лицензией Apache-2.0: все функции бесплатны, без встроенных покупок, подписок и ограничений функционала."
                }
              },
              {
                "@type": "Question",
                "name": "Какие операционные системы поддерживаются?",
                "acceptedAnswer": {
                  "@type": "Answer",
                  "text": "Windows 10 версии 1709 или новее, macOS 10.15 или новее, а также дистрибутивы Linux, например Ubuntu 22.04 или новее. Для каждой платформы есть сборки x64 и ARM64."
                }
              },
              {
                "@type": "Question",
                "name": "Как установить в Windows?",
                "acceptedAnswer": {
                  "@type": "Answer",
                  "text": "Используйте winget: выполните winget install SunJary.NetAssistant в терминале, а обновляйтесь позже командой winget upgrade. Также можно использовать установщик или портативный zip-архив."
                }
              },
              {
                "@type": "Question",
                "name": "Есть ли сборка для ARM64?",
                "acceptedAnswer": {
                  "@type": "Answer",
                  "text": "Да. Для Windows и Linux поставляются сборки aarch64, а для macOS — универсальный DMG для Intel и Apple Silicon, что покрывает Surface Pro X, Raspberry Pi 5 и Mac на чипах M-серии."
                }
              },
              {
                "@type": "Question",
                "name": "macOS сообщает, что не может проверить разработчика. Что делать?",
                "acceptedAnswer": {
                  "@type": "Answer",
                  "text": "Это нормально для неподписанных приложений без нотаризации и не означает, что приложение повреждено. Нажмите правой кнопкой мыши на NetAssistant и выберите «Открыть» либо перейдите в «Системные настройки → Конфиденциальность и безопасность» и нажмите «Всё равно открыть»."
                }
              }
            ]
          }
        ]
      }
---

# Скачать

NetAssistant доступен для Windows, Linux и macOS, со сборками x64 и ARM64 для каждой платформы. Ссылки ниже всегда ведут на файлы [последнего релиза](https://github.com/sunjary/netassistant/releases/latest) — нажмите, чтобы скачать напрямую. Старые версии можно найти на [GitHub Releases](https://github.com/sunjary/netassistant/releases).

Руководства по установке для отдельных платформ: [Windows](/en/windows-network-debug-tool) · [macOS](/en/mac-network-debug-tool) · [Linux](/en/linux-network-debug-tool).

<DownloadHero />

## Windows

**Рекомендуется: установка через winget** (с автоматическими обновлениями)

```bash
winget install SunJary.NetAssistant
```

Для обновления:

```bash
winget upgrade SunJary.NetAssistant
```

**Или установщик**: скачайте [netassistant-windows-x86_64-setup.exe](https://github.com/sunjary/netassistant/releases/latest/download/netassistant-windows-x86_64-setup.exe) и следуйте мастеру (ярлыки в меню «Пуск» и на рабочем столе создаются автоматически).

**Альтернатива**: скачайте [netassistant-windows-x86_64.zip](https://github.com/sunjary/netassistant/releases/latest/download/netassistant-windows-x86_64.zip), распакуйте и запустите `netassistant.exe`.

**Устройства ARM64** (Surface Pro X, ноутбуки на Snapdragon и т. п.): winget и установщик пока содержат только x64. На ARM64 скачайте [netassistant-windows-aarch64.zip](https://github.com/sunjary/netassistant/releases/latest/download/netassistant-windows-aarch64.zip), распакуйте и запустите `netassistant.exe`. Эта сборка создаётся автоматически и не прошла полной проверки на реальном оборудовании.

## Linux

**Рекомендуется: deb-пакет** (Debian/Ubuntu и производные, зависимости устанавливаются автоматически)

1. Скачайте [netassistant-linux-x86_64.deb](https://github.com/sunjary/netassistant/releases/latest/download/netassistant-linux-x86_64.deb)
2. Установите (зависимости разрешаются автоматически):

```bash
sudo apt install ./netassistant-linux-x86_64.deb
```

3. Запустите из меню приложений или командой `netassistant` в терминале

**AppImage** (портативная версия для Fedora/Arch и других дистрибутивов)

1. Скачайте [netassistant-linux-x86_64.AppImage](https://github.com/sunjary/netassistant/releases/latest/download/netassistant-linux-x86_64.AppImage)
2. Дайте файлу право на выполнение и запустите:

```bash
chmod +x netassistant-linux-x86_64.AppImage
./netassistant-linux-x86_64.AppImage
```

**Альтернатива: tar.gz** (лёгкий вариант)

1. Скачайте [netassistant-linux-x86_64.tar.gz](https://github.com/sunjary/netassistant/releases/latest/download/netassistant-linux-x86_64.tar.gz)
2. Распакуйте и запустите:

```bash
tar -xzf netassistant-linux-x86_64.tar.gz
chmod +x netassistant
./netassistant
```

**Устройства ARM64** (Raspberry Pi 5, ARM-серверы и т. п.): скачайте [netassistant-linux-aarch64.deb](https://github.com/sunjary/netassistant/releases/latest/download/netassistant-linux-aarch64.deb), [netassistant-linux-aarch64.AppImage](https://github.com/sunjary/netassistant/releases/latest/download/netassistant-linux-aarch64.AppImage) или [netassistant-linux-aarch64.tar.gz](https://github.com/sunjary/netassistant/releases/latest/download/netassistant-linux-aarch64.tar.gz), порядок использования такой же, как выше. Эта сборка создаётся автоматически и не прошла полной проверки на реальном оборудовании.

## macOS

**Рекомендуется: универсальный DMG** (работает и на Intel, и на Apple Silicon)

1. Скачайте [netassistant-macos-universal.dmg](https://github.com/sunjary/netassistant/releases/latest/download/netassistant-macos-universal.dmg), откройте его и перетащите NetAssistant в папку «Программы»
2. Если при первом запуске macOS предупреждает, что «разработчик не может быть проверен» (это нормально для неподписанных приложений без нотаризации, а не повреждение): нажмите правой кнопкой мыши на NetAssistant → «Открыть»; либо откройте «Системные настройки → Конфиденциальность и безопасность» и нажмите «Всё равно открыть»

**Альтернатива**: скачайте [netassistant-macos-universal.zip](https://github.com/sunjary/netassistant/releases/latest/download/netassistant-macos-universal.zip) и распакуйте для прямого запуска (права на выполнение сохраняются).

## Системные требования

| Платформа | Требования |
| --------- | ---------- |
| Windows | Windows 10 версии 1709 (сборка 16299) или новее, включая Windows 11 (x64 / ARM64). Поддерживаются Windows Server 2019 и новее; Windows Server 2016 и более старые — нет |
| Linux | Ubuntu 22.04 или новее (x86_64 / ARM64), GPU с поддержкой Vulkan |
| macOS | macOS 10.15 или новее |

## Сборка из исходников

Чтобы собрать свою версию или получить свежий снимок разработки:

```bash
git clone https://github.com/sunjary/netassistant.git
cd netassistant
cargo build --release
```

После сборки исполняемый файл находится в каталоге `target/release`.

## Устранение ошибок зависимостей

На большинстве настольных систем нужные зависимости уже есть, и всё работает сразу. В Linux устанавливайте их, только если запуск завершается ошибкой:

- AppImage завершается с ошибкой FUSE (например, `AppImages require FUSE to run`):

```bash
sudo apt install libfuse2
```

- Бинарный файл / tar.gz завершается с ошибкой об отсутствующей библиотеке GTK3 (например, `error while loading shared libraries: libgtk-3.so.0`):

```bash
sudo apt install libgtk-3-0
```

## FAQ

### NetAssistant бесплатен?

Да. NetAssistant — открытый проект под лицензией Apache-2.0: все функции бесплатны, без встроенных покупок, подписок и ограничений функционала.

### Какие операционные системы поддерживаются?

Windows 10 версии 1709 или новее, macOS 10.15 или новее, а также дистрибутивы Linux, например Ubuntu 22.04 или новее. Для каждой платформы есть сборки x64 и ARM64.

### Как установить в Windows?

Используйте winget: выполните `winget install SunJary.NetAssistant` в терминале, а обновляйтесь позже командой `winget upgrade`. Также можно использовать установщик или портативный zip-архив.

### Есть ли сборка для ARM64?

Да. Для Windows и Linux поставляются сборки aarch64, а для macOS — универсальный DMG для Intel и Apple Silicon, что покрывает Surface Pro X, Raspberry Pi 5 и Mac на чипах M-серии.

### macOS сообщает, что не может проверить разработчика. Что делать?

Это нормально для неподписанных приложений без нотаризации и не означает, что приложение повреждено. Нажмите правой кнопкой мыши на NetAssistant и выберите **«Открыть»** либо перейдите в **«Системные настройки → Конфиденциальность и безопасность»** и нажмите **«Всё равно открыть»**.

Другие вопросы — в разделе [FAQ](/ru/faq), а список изменений — в [changelog](/en/changelog).
