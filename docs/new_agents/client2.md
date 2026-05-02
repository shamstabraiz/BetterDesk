# Prompt: BetterDesk Agent Client

## Cel
Na bazie istniejącego projektu `shamstabraiz/BetterDesk` utwórz nowy, wydzielony typ klienta o nazwie **BetterDesk Agent Client** — klient instalowany na komputerze docelowym użytkownika końcowego, którego zadaniem jest umożliwienie administratorom bezpiecznego zdalnego dostępu, zbieranie informacji o urządzeniu, wykonywanie zadań automatyzacyjnych oraz zapewnienie lekkiego i możliwie dyskretnego działania.

Klient Agent ma być zoptymalizowany pod kątem:
- bezpieczeństwa,
- niezawodności,
- pracy cross-platform,
- małej ingerencji w system,
- prostoty po stronie użytkownika końcowego,
- łatwej rejestracji do serwera BetterDesk,
- zdalnego zarządzania przez klienta MGMT,
- przyszłej skalowalności i automatyzacji.

## Główna rola klienta Agent
Klient Agent ma działać jako instalowany na urządzeniu końcowym komponent, który:
1. rejestruje urządzenie w serwerze BetterDesk,
2. umożliwia administratorom zdalny dostęp do urządzenia,
3. zbiera podstawowe i rozszerzone informacje o systemie,
4. przyjmuje polecenia automatyzacji od administratorów,
5. zapewnia prosty kanał czatu z operatorem,
6. oferuje minimalistyczny interfejs użytkownika,
7. po instalacji przeprowadza bezpieczny proces konfiguracji połączenia z serwerem.

## Kluczowe założenie UX
Ten klient ma być prosty dla użytkownika końcowego. Użytkownik nie powinien mieć dostępu do rozbudowanego panelu administracyjnego. Interfejs powinien być ograniczony do minimum i skupiony na:
- statusie połączenia,
- podstawowych ustawieniach,
- czacie,
- informacji o aktywnej pomocy,
- ewentualnej zgodzie na połączenie, jeśli polityka wdrożenia tego wymaga.

Jednocześnie architektura ma pozwalać, aby klient działał:
- jako aplikacja z prostym UI,
- jako usługa/system service działająca w tle,
- opcjonalnie jako tryb bardziej ukryty/managed deployment w środowiskach firmowych, **ale wyłącznie z zachowaniem zgodności z prawem, politykami bezpieczeństwa i transparentnością wymaganą w danym środowisku**.

## Proces instalacji i rejestracji
Podczas instalacji klienta Agent ma pojawić się monit o wpisanie adresu serwera BetterDesk.

Wymagany przebieg:
1. użytkownik wpisuje adres serwera,
2. klient waliduje format adresu,
3. klient wykonuje test połączenia,
4. klient sprawdza:
   - dostępność serwera,
   - zgodność wersji/protokołu,
   - możliwość rejestracji,
   - poprawność certyfikatu / zaufania,
5. jeśli test przejdzie, klient przechodzi do bezpiecznej rejestracji,
6. po rejestracji wykonywana jest selektywna synchronizacja konfiguracji,
7. dopiero po poprawnej synchronizacji klient odblokowuje pełną funkcję zarządzania zdalnego.

Jeśli serwer nie przejdzie walidacji:
- instalacja nie powinna włączać pełnej funkcjonalności,
- użytkownik powinien dostać czytelny komunikat,
- logika ma uniemożliwiać nieautoryzowane lub błędne połączenie.

## Funkcje klienta Agent

### 1. Rejestracja i tożsamość urządzenia
Klient Agent ma:
- generować lub otrzymywać unikalny identyfikator urządzenia,
- bezpiecznie rejestrować się do serwera,
- utrzymywać trwałą, ale rotowalną tożsamość kryptograficzną,
- obsługiwać odnowienie rejestracji,
- wspierać przypisanie do grup, tenantów, polityk lub lokalizacji.

### 2. Zdalny dostęp
Klient ma pozwalać administratorom na:
- zdalny podgląd ekranu,
- zdalne sterowanie,
- przesyłanie plików, jeśli projekt to wspiera,
- czat z użytkownikiem,
- wielosesyjne scenariusze zgodnie z polityką uprawnień,
- raportowanie jakości połączenia i możliwości sprzętowych.

Agent ma wspierać:
- adaptację jakości,
- dopasowanie do słabszego łącza,
- selektywną aktywację zaawansowanych funkcji tylko po poprawnej synchronizacji z serwerem.

### 3. Zbieranie informacji o systemie
Klient Agent ma raportować – zgodnie z polityką prywatności i zakresem uprawnień – informacje takie jak:
- nazwa hosta,
- system operacyjny,
- wersja systemu,
- architektura CPU,
- pamięć RAM,
- podstawowe informacje o procesorze,
- adresy sieciowe,
- nazwa urządzenia,
- status online/offline,
- wersja klienta,
- stan usług klienckich,
- metryki diagnostyczne,
- opcjonalnie informacje o monitorach, możliwościach kodeków i akceleracji.

Nie zbieraj danych niepotrzebnych. Stosuj zasadę minimalizacji danych.

### 4. Automatyzacja administracyjna
Klient Agent ma wspierać wykonywanie zdalnie zlecanych zadań administracyjnych i automatyzacyjnych, takich jak:
- uruchamianie zatwierdzonych skryptów,
- wykonywanie poleceń administracyjnych,
- wdrażanie polityk,
- zbieranie diagnostyki,
- reagowanie na zadania zlecane przez operatorów.

Wymagania bezpieczeństwa dla automatyzacji:
- podpisywanie zadań lub bezpieczny model autoryzacji,
- walidacja źródła polecenia,
- ścisły model uprawnień,
- audyt wszystkich wykonanych działań,
- ograniczenia zakresu poleceń,
- odporność na nadużycia,
- możliwość wyłączenia lub ograniczenia funkcji przez politykę serwera.

### 5. Czat
Klient Agent ma umożliwiać prosty kontakt użytkownika z operatorem:
- wysyłanie i odbieranie wiadomości,
- powiadomienia o nowej wiadomości,
- informacja o aktywnej sesji,
- historia podstawowa lub ograniczona polityką.

### 6. Ustawienia minimalne
Interfejs użytkownika końcowego powinien udostępniać tylko podstawowe opcje:
- status połączenia,
- adres serwera,
- test połączenia,
- informacje o wersji,
- podstawowe ustawienia prywatności i zgody,
- uruchom ponownie usługę,
- wyślij diagnostykę / zgłoszenie pomocy.

Bez rozbudowanych ustawień technicznych dla użytkownika.

## Tryb działania w tle
Klient Agent powinien wspierać:
- uruchamianie jako usługa systemowa,
- autostart,
- odporność na restart systemu,
- mechanizm self-healing / watchdog jeśli architektura projektu to umożliwia,
- bezpieczne aktualizacje,
- tryb cichy dla wdrożeń firmowych.

Wszystkie funkcje “ukrytego działania” muszą być projektowane zgodnie z:
- legalnością,
- zasadami transparentności wdrożenia,
- politykami organizacji,
- wymaganiami bezpieczeństwa i zgodą administratora środowiska.

## Bezpieczeństwo — wymagania krytyczne
To ma być komponent wysokiego zaufania, więc bezpieczeństwo jest kluczowe.

Wymagania:
- bezpieczna rejestracja urządzenia,
- wzajemna autoryzacja z serwerem,
- bezpieczne przechowywanie sekretów,
- szyfrowanie komunikacji,
- pinning certyfikatów lub bezpieczny model zaufania,
- ochrona przed podszyciem się pod serwer,
- ochrona przed nieautoryzowanym sterowaniem urządzeniem,
- rotacja tokenów i sesji,
- lokalny hardening procesu,
- ograniczenie uprawnień do niezbędnego minimum,
- jawne rozróżnienie trybów attended/unattended jeśli projekt to wspiera,
- pełne logowanie audytowe działań administracyjnych,
- bezpieczny mechanizm aktualizacji,
- walidacja konfiguracji pobieranej z serwera,
- sandboxing lub ograniczanie ryzyka dla zadań automatyzacji tam, gdzie to możliwe.

## Kompatybilność systemowa
Klient Agent ma działać możliwie szeroko na:
- Windows,
- Linux,
- macOS.

Uwzględnij:
- instalatory per platforma,
- usługi systemowe,
- różnice uprawnień i UAC,
- przechwytywanie obrazu i wejścia,
- różnice w firewallu i sieci,
- różnice w autostarcie,
- przechowywanie sekretów,
- różnice w uruchamianiu skryptów PowerShell/Shell.

## Wymagania implementacyjne
Na bazie aktualnego projektu `BetterDesk`:
1. przeanalizuj istniejące moduły klienta i serwera,
2. wykorzystaj maksymalnie to, co już istnieje,
3. zaprojektuj klienta Agent jako komponent lekki i stabilny,
4. rozdziel część UI od części service/daemon,
5. wykorzystaj Go tam, gdzie potrzebna jest stabilna warstwa systemowa, komunikacyjna i serwisowa,
6. wykorzystaj JavaScript/HTML/CSS tam, gdzie obecna architektura projektu już to wspiera,
7. przygotuj strukturę modułów odpowiednią do długiego utrzymania.

## Synchronizacja po rejestracji
Po pierwszym połączeniu klient nie powinien od razu aktywować wszystkich funkcji. Ma działać etapowo:
- etap 1: walidacja serwera,
- etap 2: rejestracja,
- etap 3: podstawowa synchronizacja konfiguracji,
- etap 4: weryfikacja polityk i uprawnień,
- etap 5: aktywacja funkcji zdalnego zarządzania,
- etap 6: okresowa synchronizacja selektywna.

Synchronizacja ma być:
- odporna na błędy,
- wznawialna,
- bezpieczna,
- audytowalna,
- minimalna pod względem ilości przesyłanych danych.

## Testy i jakość
Przygotuj rozwiązanie wraz z planem:
- testów jednostkowych,
- testów integracyjnych,
- testów rejestracji klienta,
- testów bezpieczeństwa,
- testów automatyzacji,
- testów kompatybilności systemowej,
- testów aktualizacji,
- testów odporności na utratę połączenia,
- testów rekonfiguracji po zmianie serwera lub certyfikatu.

## Oczekiwany rezultat
Wygeneruj kompletną koncepcję i implementację **BetterDesk Agent Client**, obejmującą:
- architekturę,
- strukturę katalogów,
- proces instalacji,
- onboarding serwera,
- walidację połączenia,
- bezpieczną rejestrację,
- mechanizm synchronizacji,
- zdalny dostęp,
- moduł zbierania informacji o systemie,
- moduł automatyzacji,
- moduł czatu,
- minimalny interfejs użytkownika,
- tryb usługi/system daemon,
- mechanizmy bezpieczeństwa,
- strategię budowania i wdrożenia cross-platform.

Jeżeli repozytorium posiada ograniczenia uniemożliwiające pełną realizację któregoś elementu, opisz je uczciwie i zaproponuj najlepszą możliwą architekturę pośrednią zgodną z obecnym stanem projektu.