# Сообщения нативного интерфейса. Идентификаторы и имена переменных не переводятся.
# Пользовательские значения, пути, идентификаторы протокола и машинный вывод не переводятся.

-brand-name = Sniptale
sandbox-title = { -brand-name } — Песочница нативных компонентов · { $version }
sandbox-heading = { -brand-name } / Компоненты
sandbox-description = Нативный образец · данные в памяти · без захвата и записи настроек
sandbox-theme-label = Тема
sandbox-language-label = Язык
theme-light = Светлая
theme-dark = Тёмная
theme-contrast = Системные контрастные цвета
theme-change-failed = Не удалось изменить тему. Предыдущая тема сохранена. Попробуйте ещё раз.
language-system = Язык системы
navigation-capture = Захват
navigation-video = Видео
command-capture-fixture = Захват (образец)
command-secondary = Образец меню…
command-reset-counter = Сбросить счётчик
commands-heading = Команды · нейтральная иерархия
values-heading = Черновик настроек
field-include-cursor = Показывать курсор
field-pending-fixture = Имитировать выполнение действия
field-save-folder-fixture = Папка сохранения (пример)
field-hotkey-specimen = Сочетание · Escape отменяет; Delete очищает
shortcut-press = Нажмите сочетание…
shortcut-accepted = Сочетание сохранено в памяти. Глобальное сочетание не зарегистрировано.
shortcut-cancelled = Ввод отменён. Прежнее значение сохранено.
shortcut-cleared = Сочетание очищено в образце.
shortcut-unset = Задать сочетание…
shortcut-invalid = Добавьте Ctrl, Shift, Alt или Win к букве или цифре.
shortcut-conflict = Ctrl+Shift+V зарезервировано в образце. Выберите другое сочетание.
feedback-ready = Готово. Проверьте Tab, пробел, нажатие, перетаскивание и смену темы.
feedback-counter-reset = Готово. Счётчик сброшен.
feedback-capture-selected = Выбран раздел захвата.
feedback-video-selected = Выбран раздел видео.
feedback-activation-count =
    { $count ->
        [one] Выполнено { $count } действие. Образец не захватывал медиа.
        [few] Выполнено { $count } действия. Образец не захватывал медиа.
       *[many] Выполнено { $count } действий. Образец не захватывал медиа.
    }
sandbox-footer = { $version } · { $locale } · образец нативных компонентов · docs/design/foundations.md

menu-mode = Режим
menu-local = Локальный захват
menu-browser = Назначение в браузере (образец)
menu-unavailable = Недоступно в образце
menu-selected = Пункт меню выбран. Команда приложению не отправлена.
menu-cancelled = Меню закрыто. Команда приложению не отправлена.

panel-all-screens = Все экраны
panel-busy = Выполняется захват. Подождите.
panel-capture = Сделать скриншот
panel-discard = Удалить запись
panel-finalizing = Сохранение записи…
panel-menu = Другие действия
panel-pause = Пауза
panel-paused = Запись приостановлена
panel-preparing = Подготовка записи…
panel-ready = Всё готово к захвату
panel-record = Начать запись
panel-recording = Идёт запись
panel-region = Область
panel-resume = Продолжить
panel-screen = Текущий экран
panel-screenshot = Скриншот
panel-select-region = Выберите область после запуска захвата.
panel-select-window = Будет захвачено окно, которое было активно до открытия панели.
panel-settings = Настройки
panel-stop = Остановить и сохранить
panel-target-label = Область захвата
panel-unavailable = Захват недоступен. Проверьте папку сохранения в настройках.
panel-video = Видео
panel-window = Активное окно

sandbox-scene-label = Просмотр
sandbox-scene-components = Компоненты
sandbox-scene-capture-panel = Панель захвата

panel-screenshot-done = Скриншот готов (предпросмотр).
panel-saved = Запись сохранена (предпросмотр).
panel-discarded = Запись отменена (предпросмотр).

choice-unavailable = { $label } (недоступно)
encoder-prefer = Предпочитать аппаратный
encoder-software = Программный
feedback-appearance-failed = Не удалось сохранить оформление. Предыдущая настройка сохранена.
feedback-audio-unavailable = Этот источник звука недоступен. Выберите другой.
feedback-hotkey-unavailable = Это сочетание недоступно. Выберите другое.
feedback-microphone-unavailable = Выбранный микрофон недоступен. Выберите другое устройство или обновите список.
feedback-recording-busy = Остановите запись перед применением изменений.
feedback-save-failed = Не удалось сохранить изменения. Продолжите редактирование и повторите попытку.
feedback-shortcut-conflict = Выберите другое сочетание. Оно уже назначено действию «{ $action }».
microphone-name = { $status ->
    [default] { $name } (по умолчанию)
    [unavailable] { $name } (недоступен)
    [default-unavailable] { $name } (по умолчанию, недоступен)
   *[available] { $name }
    }
option-automatic = Автоматически
quality-balanced = Сбалансированное
quality-high = Высокое
quality-low = Низкое
quality-ultra = Максимальное
settings-apply = Применить
settings-apply-extension = Перейти в режим расширения
settings-apply-manual = Сохранить и включить локальный захват
settings-ask-destination = Спрашивать, куда сохранить, после каждого захвата
settings-audio = Звук
settings-audio-description = Выберите, какие звуки записывать.
settings-audio-microphone = Микрофон
settings-audio-mixed = Системный звук и микрофон
settings-audio-quality = Битрейт звука
settings-audio-source = Источник
settings-audio-system = Системный звук
settings-cancel = Отмена
settings-capture = Захват
settings-change-folder = Изменить папку…
settings-choose-local-folder = Выбрать локальную папку…
settings-delivery = Куда отправлять результаты
settings-description = Захват, запись и сочетания клавиш
settings-device-default = Системное устройство
settings-device-missing = Ранее выбранный микрофон (недоступен)
settings-dirty-description = Изменения ещё не применены. Продолжите редактирование или сбросьте их.
settings-discard = Сбросить изменения
settings-duration = Максимальная длительность
settings-encoder = Кодировщик
settings-extension-description = Новые снимки и записи будут использовать настройки подключённого расширения браузера.
settings-extension-note = В режиме расширения локальные сочетания клавиш отключены.
settings-extension-title = Перейти в режим расширения?
settings-frame-rate = Частота кадров
settings-heading = Настройки
settings-keep-editing = Продолжить редактирование
settings-local-description = Сохраняйте результаты на компьютере, даже когда браузер закрыт.
settings-local-folder = Локальная папка
settings-microphone = Микрофон
settings-pointer-highlights = Собирать данные для подсветки указателя и нажатий
settings-quality = Качество
settings-refresh-browsers = Обновить список браузеров
settings-refresh-devices = Обновить устройства
settings-save-delivery = Сохранить выбор назначения
settings-save-location = Папка сохранения
settings-screenshot-cursor = Показывать указатель на скриншотах
settings-screenshots = Скриншоты
settings-setup-description = Выберите локальную папку перед первым захватом. Результаты также можно отправлять в доверенный браузер.
settings-setup-note = Локальная папка сохраняет доступ к результатам, когда браузер закрыт.
settings-setup-title = Выберите папку сохранения
settings-shortcuts = Сочетания клавиш
settings-shortcuts-description = Выберите поле и нажмите сочетание клавиш. Для отмены нажмите Esc.
settings-system-theme = Как в системе
settings-theme = Тема
settings-use-extension = Режим расширения
settings-video = Видео
settings-video-bitrate = Битрейт видео
settings-video-cursor = Показывать указатель в видео
settings-video-description = Выберите качество и длительность записи.
shortcut-capturing = Нажмите сочетание…
shortcut-not-set = Не задано
shortcut-pause-resume = Пауза или продолжение записи
shortcut-record-region = Запись области
shortcut-record-screen = Запись экрана
shortcut-record-window = Запись активного окна
shortcut-region = Снимок области
shortcut-screen = Снимок экрана
shortcut-stop = Остановка записи
shortcut-window = Снимок активного окна
value-fps = { $value } кадр/с
value-hours = { $count ->
    [one] { $count } час
    [few] { $count } часа
   *[many] { $count } часов
    }
value-kbps = { $value } кбит/с
value-mbps = { $value } Мбит/с
value-minutes = { $count ->
    [one] { $count } минута
    [few] { $count } минуты
   *[many] { $count } минут
    }
value-seconds = { $count ->
    [one] { $count } секунда
    [few] { $count } секунды
   *[many] { $count } секунд
    }

settings-choose-folder = Выбрать папку

shortcut-needs-modifier = Добавьте модификатор

settings-title = Sniptale — Настройки

panel-title = Захват Sniptale
panel-needs-destination = Перед захватом выберите папку сохранения в настройках.
panel-setup = Настроить захват

# $state is localized recording status. $time is active recording time in mm:ss.
panel-recording-time = { $state } · { $time }

panel-no-window = Сначала активируйте нужное окно, затем снова откройте эту панель.

# Общие сообщения web-интерфейса для песочницы и Tauri.
web-theme = Оформление
web-system = Системная
web-language = Язык
web-capture-toolbar = Панель захвата
web-capture-kind = Тип захвата
web-move-panel = Переместить панель захвата
web-hide-panel = Скрыть панель захвата
web-not-supported = Этот backend не поддерживает захват. Захват не запустится
web-connection-failed = Нет связи с агентом. Откройте панель заново для подключения
web-phase-selecting = Выберите область…
web-phase-starting = Запуск…
web-phase-saving = Сохранение…
web-paused = Пауза
web-stop = Стоп
web-outcome-saved = Захват сохранён
web-outcome-discarded = Запись удалена
web-outcome-permission-denied = Нет разрешения на захват. Разрешите запись экрана в системных настройках и повторите попытку
web-outcome-save-failed = Не удалось сохранить захват. Проверьте папку и свободное место, затем повторите попытку
web-discard-title = Удалить эту запись?
web-discard-body = Эта запись не будет сохранена. Вместо удаления можно продолжить запись.
web-design-system = Дизайн-система
web-sandbox = Песочница компонентов
web-sandbox-note = Тестовый backend. Захват экрана и запись файлов не выполняются.
web-toolbar-title = Панель захвата
web-toolbar-description = Выберите тип и источник. Запустите захват с одной плавающей панели.
web-scenario = Сценарий
web-scenario-success = Успешный захват
web-scenario-permission-denied = Нет разрешения
web-scenario-save-failed = Ошибка сохранения
web-scenario-unsupported = Захват не поддерживается
web-reset = Сбросить пример
web-open-panel = Показать панель
web-preview = Предпросмотр
web-preview-description = Приложение использует эту панель и её компоненты.
web-contract-title = Один интерфейс, два клиента.
web-contract-description = Этот пример использует управляемые фикстуры. Приложение — типизированный клиент Tauri. Захватом, записью и файлами управляет Rust.
web-desktop-required = Откройте интерфейс в приложении или перейдите на /sandbox для просмотра тестовых компонентов.

web-text-preview = Проверка текста
web-text-normal = Обычный текст
web-text-expanded = Удлинённый текст
web-text-rtl = Справа налево (тест)
web-text-expanded-rtl = Длинный текст справа налево (тест)
web-text-scale = Масштаб панели

web-scenario-pending = Захват выполняется

web-switch-video = Переключить на видео
web-switch-screenshot = Переключить на скриншот
web-capture-options = Параметры захвата
web-recording-quality = Качество видео
web-quality-next-recording = Для следующей записи
web-app-settings = Настройки приложения
web-app-settings-description = Оформление и язык всего приложения
web-preferences-failed = Не удалось сохранить настройки. Повторите попытку

web-outcome-capture-failed = Не удалось выполнить захват. Попробуйте снова
web-outcome-cancelled = Захват отменён

web-general = Общие
web-general-description = Оформление и язык меняются сразу
web-settings-sections = Разделы настроек
web-bitrate-automatic = Оставьте поле пустым для автоматической настройки качества
web-seconds-short = сек
web-pointer-highlights-detail = Дополнительные данные о движении указателя и кликах
web-settings-unavailable = Настройки недоступны · изменения сохранены в форме
web-settings-saving = Применяем изменения…
web-settings-unsaved = Есть неприменённые изменения
web-settings-up-to-date = Изменения применены
web-settings-preview = Пример настроек · тестовые данные
web-preview-microphone = Тестовый микрофон
web-settings-scenario-ready = Готово
web-settings-scenario-save-failed = Ошибка сохранения
web-settings-scenario-pending = Применение
web-settings-scenario-unavailable = Недоступно

web-settings-stale = Настройки изменились. Откройте их заново перед применением изменений
web-settings-invalid = Проверьте папку, числовые значения и сочетания клавиш и повторите попытку
web-settings-load-failed = Не удалось загрузить настройки
web-settings-loading = Загрузка настроек…
web-retry = Повторить

web-delivery-description = Выберите зарегистрированный браузер для результатов локального захвата
web-delivery-empty = Подключите расширение Sniptale, чтобы добавить браузер
web-delivery-missing = Получатель больше недоступен
web-preview-browser = Тестовый браузер — Личный

web-setup-required = Перед захватом выберите папку в настройках приложения
web-command-busy = Другое действие захвата ещё выполняется. Дождитесь завершения и повторите попытку
web-command-rejected = Состояние захвата изменилось. Проверьте доступные действия и повторите попытку
web-command-unavailable = Действие захвата недоступно. Повторите попытку
web-capture-unavailable = Захват временно недоступен
web-scenario-setup-required = Требуется папка сохранения
web-scenario-command-refused = Команда отклонена
web-scenario-connection-failed = Ошибка соединения

web-window-action-failed = Действие окна недоступно

web-window-unavailable = Активируйте нужное окно и вернитесь к панели
web-command-source-unavailable = Нет доступного окна захвата. Активируйте нужное окно и повторите попытку
web-scenario-window-lost = Окно стало недоступно

native-settings-title = Настройки Sniptale
native-capture-title = Захват Sniptale

web-dialog-connection-title = Разрешить подключение браузера?
web-dialog-connection-body = Разрешить этому расширению и приложению запускать соединение с установленным Sniptale
web-dialog-extension = Расширение
web-dialog-launcher = Приложение
web-dialog-launcher-diagnostic = Сведения о приложении служат для диагностики и не подтверждают подлинность браузера
web-dialog-allow = Разрешить
web-dialog-deny = Отклонить
web-dialog-destination-title = Куда сохранить захват?
web-dialog-destination-body = Локальная копия сохранена в выбранной папке. Можно также отправить её в браузер
web-dialog-destination-retained = Сохранить в папку не удалось. Захват сохранён для восстановления. Отправьте его в браузер или оставьте на потом
web-dialog-local-folder = Локальная папка
web-dialog-later = Оставить на потом
web-dialog-quit-recording-title = Остановить запись и выйти?
web-dialog-quit-recording-body = Sniptale остановит и сохранит текущую запись перед выходом
web-dialog-stop-exit = Сохранить и выйти
web-dialog-unavailable = Диалог больше недоступен. Закройте его и повторите действие
