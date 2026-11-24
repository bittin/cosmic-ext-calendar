app-title = Kalendarz
about = O kalendarzu
about-description = Planuj wydarzenia i organizuj swój harmonogram w środowisku COSMIC.
repository = Repozytorium
about-releases = Wydania
about-report-issue = Zgłoś problem
app-menu = Menu aplikacji
page-id = Strona { $num }
git-description = Git commit {$hash} z {$date}

today = Dzisiaj
month = Miesiąc
week = Tydzień
day = Dzień

create-event = Utwórz wydarzenie

previous-month = Poprzedni miesiąc
next-month = Następny miesiąc
new-event = Nowe wydarzenie
all-day = Cały dzień
month-day-agenda = Plan dnia
month-agenda-date = { $weekday }, { $day } { $month }
month-no-events = Brak wydarzeń
month-compact-event-count = +{ $count }
month-more-events = +{ $count } więcej
month-day-accessible =
    { $count ->
        [one] { $date }, jedno wydarzenie
        [few] { $date }, { $count } wydarzenia
       *[many] { $date }, { $count } wydarzeń
    }
weekday-mon = pon.
weekday-tue = wt.
weekday-wed = śr.
weekday-thu = czw.
weekday-fri = pt.
weekday-sat = sob.
weekday-sun = niedz.
month-jan = Styczeń
month-feb = Luty
month-mar = Marzec
month-apr = Kwiecień
month-may = Maj
month-jun = Czerwiec
month-jul = Lipiec
month-aug = Sierpień
month-sep = Wrzesień
month-oct = Październik
month-nov = Listopad
month-dec = Grudzień

start-date = Data rozpoczęcia
end-date = Data zakończenia
invalid-date-range = Godzina zakończenia nie może być wcześniejsza niż godzina rozpoczęcia.
invalid-all-day-range = Wybierz datę zakończenia przypadającą w dniu rozpoczęcia lub później.
event-timezone = Strefa czasowa
event-timezone-placeholder = Opcjonalnie, np. Europe/Warsaw
invalid-event-timezone = Wprowadź strefę czasową, np. Europe/Warsaw.
nonexistent-event-time = Ta godzina nie występuje w wybranej strefie czasowej. Wybierz inną godzinę.
event-save-error = Nie udało się zapisać wydarzenia. Zmiany są nadal w edytorze.
event-delete-error = Nie udało się usunąć wydarzenia. { $error }
settings = Ustawienia
settings-date-and-time = Data i czas
display-timezone = Strefa czasowa widoku
display-timezone-system = Systemowa strefa czasowa
recurrence-edit-scope = Zastosuj zmiany do
recurrence-this-event = To wydarzenie
recurrence-all-events = Wszystkie wydarzenia
ics-import-result = Zaimportowano { $imported }. Pominięto { $skipped } duplikatów lub nieobsługiwanych pozycji.
ics-import-error = Nie udało się ukończyć importu. { $error }
ics-import-working = Importowanie kalendarza…
ics-import-no-calendar = Utwórz kalendarz lokalny przed importem.
ics-import-destination = Kalendarz docelowy
ics-import-confirm = Importuj
delete-following = Usuń to i kolejne
recurrence-following-events = To i kolejne wydarzenia
event-floating-time = Zachowaj tę samą godzinę w każdej strefie czasowej
calendar-add-at = Utwórz wydarzenie { $date } o { $time }
event-writing = Zapisywanie zmian…
invalid-recurrence-end = Data zakończenia powtarzania nie może być wcześniejsza niż data rozpoczęcia.
calendar-copy = Kopiuj
calendar-cut = Wytnij
calendar-paste = Wklej
calendar-duplicate = Powiel
calendar-paste-error = Nie udało się umieścić wydarzenia w wybranym terminie.

delete-calendar = Usuń kalendarz
archive-calendar-confirmation = Usunąć „{ $name }” z aktywnych kalendarzy? Kalendarz i wszystkie jego wydarzenia można przywrócić z archiwum kalendarzy.
archived-calendars = Archiwum kalendarzy
no-archived-calendars = Brak zarchiwizowanych kalendarzy.
restore-calendar = Przywróć
calendar-archived = Kalendarz przeniesiono do archiwum.
calendar-archive-error = Nie udało się zmienić stanu archiwizacji kalendarza. { $error }
calendar-storage-load-error = Nie udało się wczytać części danych kalendarza. Zmiany są wyłączone do czasu rozwiązania problemu i ponownego uruchomienia aplikacji. { $error }
default-calendar-error = Nie udało się utworzyć kalendarza osobistego. { $error }
calendar-create-error = Nie udało się utworzyć kalendarza. { $error }
calendar-save-error = Nie udało się zapisać kalendarza. { $error }
calendar-visibility-error = Nie udało się zmienić widoczności kalendarza. { $error }
calendar-export-error = Nie udało się wyeksportować wydarzeń kalendarza. { $error }
display-timezone-save-error = Nie udało się zapisać strefy czasowej wyświetlania. { $error }
appearance = Wygląd
theme = Motyw
theme-match-desktop = Zgodny z pulpitem
theme-dark = Ciemny
theme-light = Jasny
theme-save-error = Nie udało się zapisać motywu. { $error }

calendar-name = Nazwa kalendarza
calendar-color = Kolor kalendarza
calendar-color-swatch = Kolor { $color }
calendar-color-custom = Własny kolor
calendar-color-default = Domyślny kolor
calendar-color-recent = Ostatnie kolory
calendar-color-copy = Kopiuj do schowka
calendar-color-copied = Skopiowano do schowka

event-title = Tytuł wydarzenia
event-date-label = { $date } { $year }
event-invalid-clock = Wpisz prawidłową godzinę w formacie GG:MM.
event-calendar = Kalendarz
event-notes = Notatki
event-more-options = Więcej opcji

location-suggestions = Sugestie lokalizacji
online-location-enable = Włącz wyszukiwanie online
online-location-disable = Wyłącz wyszukiwanie online
online-location-save-error = Nie udało się zapisać ustawień wyszukiwania lokalizacji online.
online-location-unavailable = Sugestie online są niedostępne.
online-location-description = Użyj Photon do wyszukiwania adresów i miejsc. Po włączeniu zapytania są wysyłane do poniższego serwera. Publiczny serwer demonstracyjny jest przeznaczony do sporadycznego użytku.
photon-endpoint = Adres URL serwera Photon
photon-endpoint-invalid = Wpisz adres URL serwera Photon z protokołem HTTP lub HTTPS.
online-location-update = Zmień serwer
online-location-about = Informacje o serwerze publicznym
online-location-attribution = Photon · © Autorzy OpenStreetMap

event-alerts = Alerty
event-alert-at-start = W chwili rozpoczęcia
event-alert-custom = Inny czas…
event-alert-custom-minutes = Minuty przed
event-alert-invalid = Wpisz prawidłową liczbę minut przed wydarzeniem.
event-alert-duplicate = Wybierz inny czas dla każdego alertu.
event-alert-minutes =
    { $count ->
        [one] { $count } minutę przed
        [few] { $count } minuty przed
       *[other] { $count } minut przed
    }
event-alert-hours =
    { $count ->
        [one] { $count } godzinę przed
        [few] { $count } godziny przed
       *[other] { $count } godzin przed
    }
event-alert-days =
    { $count ->
        [one] { $count } dzień przed
       *[other] { $count } dni przed
    }
event-alert-none = Brak
event-alert-first = Alert
event-alert-numbered = { $number }. alert

notification-untitled-event = Wydarzenie bez tytułu

accounts = Konta
manage-accounts = Zarządzaj kontami
manage-accounts-description = Dodawaj i konfiguruj połączenia kalendarza w Kontach.
open = Otwórz
accounts-checking = Sprawdzanie kont…
accounts-daemon-unavailable = Konta są niedostępne.
no-accounts-connected = Nie podłączono żadnych kont kalendarza.
accounts-app-launch-error = Nie udało się otworzyć Kont: {$error}
account-save-error = Nie udało się zapisać ustawienia konta: {$error}
accounts-error-unavailable = Konta są niedostępne.
accounts-error-upgrade = Konta wymagają aktualizacji.
accounts-error-disabled = To konto lub jego usługa Kalendarz jest wyłączona.
accounts-error-needs-reauth = To konto wymaga ponownego połączenia.
accounts-error-access-denied = Konta odmówiły dostępu Kalendarzowi. Sprawdź uprawnienia aplikacji w Kontach.
accounts-error-server-denied = Serwer CalDAV odmówił dostępu. Sprawdź uprawnienia konta na serwerze.
accounts-error-google-denied = Google odmówiło dostępu CalDAV. Sprawdź, czy interfejs CalDAV API jest włączony w projekcie OAuth, i połącz ponownie konto Google w Kontach.
accounts-error-google-api-disabled = Włącz interfejs CalDAV API w projekcie Google Cloud używanym przez Konta, a następnie ponów synchronizację.
accounts-error-google-scope = Połącz ponownie konto Google w Kontach i udziel dostępu do Kalendarza na ekranie zgody Google.
accounts-error-consent-timeout = Żądanie Kont upłynęło.
accounts-error-failed = Konta nie mogły wykonać żądania.
accounts-error-unsupported = To konto nie jest obsługiwane.
accounts-syncing = Synchronizowanie kalendarzy…
accounts-no-calendars = Nie znaleziono kalendarzy.
accounts-sync-now = Synchronizuj teraz
accounts-error-microsoft-graph = Synchronizacja kalendarza Microsoft wymaga obsługi Microsoft Graph.

keyboard-shortcuts = Skróty klawiszowe
shortcut-general = Ogólne
shortcut-navigation = Nawigacja
shortcut-selected-event = Wydarzenia kalendarza
shortcut-event-editor = Edytor wydarzeń
shortcut-calendar-management = Zarządzanie kalendarzami
shortcut-quit = Zakończ
shortcut-refresh = Odśwież kalendarze online
shortcut-close-window = Zamknij okno
shortcut-dismiss = Zamknij lub wyczyść wyszukiwanie
shortcut-close-dialog = Zamknij okno dialogowe lub panel
shortcut-previous-day = Poprzedni dzień
shortcut-next-day = Następny dzień
shortcut-previous-week = Poprzedni tydzień
shortcut-next-week = Następny tydzień
shortcut-previous-month = Poprzedni miesiąc
shortcut-next-month = Następny miesiąc
shortcut-previous-year = Poprzedni rok
shortcut-next-year = Następny rok
shortcut-event-details = Otwórz szczegóły wybranego wydarzenia
shortcut-save-event = Zapisz wydarzenie
shortcut-toggle-all-day = Przełącz wydarzenie całodniowe
shortcut-save-calendar = Zapisz kalendarz
shortcut-confirm-archive = Potwierdź archiwizację kalendarza
shortcut-confirm-import = Potwierdź import
shortcut-toggle-sidebar = Przełącz panel kalendarzy
shortcut-month-agenda = Otwórz agendę wybranego dnia w widoku miesiąca
shortcut-previous-event = Poprzednie wydarzenie wybranego dnia
shortcut-next-event = Następne wydarzenie wybranego dnia

local-calendars = Kalendarze lokalne
