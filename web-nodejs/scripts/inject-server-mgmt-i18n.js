#!/usr/bin/env node
/**
 * One-shot script: inject Server Management i18n keys into all language files.
 * Run from repo root: node web-nodejs/scripts/inject-server-mgmt-i18n.js
 *
 * Adds:
 *   nav.server_management
 *   server_mgmt.* (full namespace)
 *
 * For languages without an explicit translation block, English is used as
 * fallback (standard i18n practice — the i18n middleware also falls back to EN).
 */

'use strict';

const fs = require('fs');
const path = require('path');

const LANG_DIR = path.join(__dirname, '..', 'lang');

// English baseline (source of truth)
const EN = {
    nav_label: 'Server Management',
    sm: {
        title: 'Server Management',
        subtitle: 'Cockpit-like control plane for the BetterDesk console host (BETA).',
        beta_tooltip: 'Beta feature — under active development.',
        notice_title: 'Beta feature',
        notice_body: 'These tools run on the host where the BetterDesk console process lives. All operations are audited. Feature scope will expand in future releases.',
        tab_overview: 'Overview',
        tab_terminal: 'Terminal',
        tab_files: 'Files',
        tab_services: 'Services',
        cpu: 'CPU',
        memory: 'Memory',
        load: 'Load average',
        disks: 'Disks',
        history: 'History (last 60 samples)',
        host_info: 'Host info',
        cores: 'cores',
        uptime: 'Uptime',
        no_disks: 'No disk data available',
        info_hostname: 'Hostname',
        info_platform: 'Platform',
        info_kernel: 'Kernel',
        info_node: 'Node.js',
        info_process_id: 'Process ID',
        info_pty_available: 'PTY available',
        term_connect: 'Connect',
        term_disconnect: 'Disconnect',
        term_clear: 'Clear',
        term_status: 'Status',
        term_connecting: 'Connecting…',
        term_connected: 'Connected',
        term_disconnected: 'Disconnected',
        term_error: 'Connection error',
        term_warning: 'Commands run as the user that owns the console process. Add a sudoers rule to enable elevation.',
        term_hint: 'Tip: type `sudo -i` to escalate (requires sudoers configuration).',
        term_lib_failed: 'Failed to load terminal library',
        files_up: 'Up one level',
        files_go: 'Go',
        files_mkdir: 'New folder',
        files_name: 'Name',
        files_size: 'Size',
        files_perms: 'Mode',
        files_mtime: 'Modified',
        files_rename: 'Rename',
        files_rename_prompt: 'New name:',
        files_empty: 'Empty directory',
        files_error: 'Failed to list directory',
        read_failed: 'Failed to read file',
        save_failed: 'Failed to save file',
        rename_failed: 'Rename failed',
        delete_failed: 'Delete failed',
        delete_confirm: 'Delete this entry?',
        mkdir_prompt: 'New folder name:',
        mkdir_failed: 'Failed to create folder',
        file_saved: 'File saved',
        binary_warn: 'Binary file — preview only',
        encoding_utf8: 'UTF-8 text',
        truncated: 'truncated',
        full: 'full',
        svc_search_placeholder: 'Search services…',
        svc_name: 'Service',
        svc_state: 'State',
        svc_description: 'Description',
        svc_empty: 'No services found',
        svc_load_failed: 'Failed to load services',
        svc_action_failed: 'Action failed',
        svc_confirm: 'Run',
        svc_start: 'Start',
        svc_stop: 'Stop',
        svc_restart: 'Restart',
        svc_reload: 'Reload',
        svc_enable: 'Enable',
        svc_disable: 'Disable',
        svc_status: 'Status'
    }
};

// Per-language overrides. Languages not listed here keep the English text.
const TRANSLATIONS = {
    pl: {
        nav_label: 'Zarządzanie serwerem',
        sm: {
            title: 'Zarządzanie serwerem',
            subtitle: 'Panel typu Cockpit dla hosta konsoli BetterDesk (BETA).',
            beta_tooltip: 'Funkcja beta — w aktywnym rozwoju.',
            notice_title: 'Funkcja w wersji beta',
            notice_body: 'Te narzędzia działają na hoście, na którym uruchomiony jest proces konsoli BetterDesk. Wszystkie operacje są audytowane. Zakres funkcji będzie rozszerzany w kolejnych wydaniach.',
            tab_overview: 'Przegląd',
            tab_terminal: 'Terminal',
            tab_files: 'Pliki',
            tab_services: 'Usługi',
            cpu: 'CPU',
            memory: 'Pamięć',
            load: 'Średnie obciążenie',
            disks: 'Dyski',
            history: 'Historia (ostatnie 60 próbek)',
            host_info: 'Informacje o hoście',
            cores: 'rdzeni',
            uptime: 'Czas pracy',
            no_disks: 'Brak danych o dyskach',
            info_hostname: 'Nazwa hosta',
            info_platform: 'Platforma',
            info_kernel: 'Jądro',
            info_node: 'Node.js',
            info_process_id: 'ID procesu',
            info_pty_available: 'PTY dostępne',
            term_connect: 'Połącz',
            term_disconnect: 'Rozłącz',
            term_clear: 'Wyczyść',
            term_status: 'Status',
            term_connecting: 'Łączenie…',
            term_connected: 'Połączono',
            term_disconnected: 'Rozłączono',
            term_error: 'Błąd połączenia',
            term_warning: 'Polecenia wykonywane są jako użytkownik procesu konsoli. Dodaj regułę sudoers, aby umożliwić eskalację uprawnień.',
            term_hint: 'Wskazówka: wpisz `sudo -i`, aby uzyskać uprawnienia root (wymaga konfiguracji sudoers).',
            term_lib_failed: 'Nie udało się załadować biblioteki terminala',
            files_up: 'Poziom wyżej',
            files_go: 'Idź',
            files_mkdir: 'Nowy folder',
            files_name: 'Nazwa',
            files_size: 'Rozmiar',
            files_perms: 'Uprawnienia',
            files_mtime: 'Zmodyfikowano',
            files_rename: 'Zmień nazwę',
            files_rename_prompt: 'Nowa nazwa:',
            files_empty: 'Pusty katalog',
            files_error: 'Nie udało się odczytać katalogu',
            read_failed: 'Nie udało się odczytać pliku',
            save_failed: 'Nie udało się zapisać pliku',
            rename_failed: 'Nie udało się zmienić nazwy',
            delete_failed: 'Nie udało się usunąć',
            delete_confirm: 'Usunąć tę pozycję?',
            mkdir_prompt: 'Nazwa nowego folderu:',
            mkdir_failed: 'Nie udało się utworzyć folderu',
            file_saved: 'Plik zapisany',
            binary_warn: 'Plik binarny — tylko podgląd',
            encoding_utf8: 'Tekst UTF-8',
            truncated: 'obcięty',
            full: 'pełny',
            svc_search_placeholder: 'Szukaj usług…',
            svc_name: 'Usługa',
            svc_state: 'Stan',
            svc_description: 'Opis',
            svc_empty: 'Nie znaleziono usług',
            svc_load_failed: 'Nie udało się załadować usług',
            svc_action_failed: 'Akcja nie powiodła się',
            svc_confirm: 'Uruchomić',
            svc_start: 'Start',
            svc_stop: 'Stop',
            svc_restart: 'Restart',
            svc_reload: 'Przeładuj',
            svc_enable: 'Włącz',
            svc_disable: 'Wyłącz',
            svc_status: 'Status'
        }
    },
    de: {
        nav_label: 'Server-Verwaltung',
        sm: {
            title: 'Server-Verwaltung',
            subtitle: 'Cockpit-ähnliche Steuerung für den BetterDesk-Konsolen-Host (BETA).',
            beta_tooltip: 'Beta-Funktion — in aktiver Entwicklung.',
            notice_title: 'Beta-Funktion',
            notice_body: 'Diese Werkzeuge laufen auf dem Host, auf dem der BetterDesk-Konsolenprozess ausgeführt wird. Alle Operationen werden auditiert.',
            tab_overview: 'Übersicht',
            tab_terminal: 'Terminal',
            tab_files: 'Dateien',
            tab_services: 'Dienste',
            cpu: 'CPU',
            memory: 'Speicher',
            load: 'Last (Durchschnitt)',
            disks: 'Festplatten',
            history: 'Verlauf (letzte 60 Werte)',
            host_info: 'Host-Info',
            cores: 'Kerne',
            uptime: 'Laufzeit',
            term_connect: 'Verbinden',
            term_disconnect: 'Trennen',
            term_clear: 'Löschen',
            term_connecting: 'Verbinden…',
            term_connected: 'Verbunden',
            term_disconnected: 'Getrennt',
            term_error: 'Verbindungsfehler',
            term_warning: 'Befehle werden als Konsolen-Prozessbenutzer ausgeführt. Sudoers-Regel hinzufügen für Erhöhung.',
            files_name: 'Name',
            files_size: 'Größe',
            files_perms: 'Modus',
            files_mtime: 'Geändert',
            files_rename: 'Umbenennen',
            files_empty: 'Leeres Verzeichnis',
            svc_name: 'Dienst',
            svc_state: 'Status',
            svc_description: 'Beschreibung',
            svc_start: 'Start',
            svc_stop: 'Stopp',
            svc_restart: 'Neustart',
            svc_reload: 'Neu laden',
            svc_enable: 'Aktivieren',
            svc_disable: 'Deaktivieren'
        }
    },
    es: {
        nav_label: 'Gestión del servidor',
        sm: {
            title: 'Gestión del servidor',
            subtitle: 'Panel tipo Cockpit para el host de la consola BetterDesk (BETA).',
            beta_tooltip: 'Función beta — en desarrollo activo.',
            notice_title: 'Función beta',
            notice_body: 'Estas herramientas se ejecutan en el host donde corre el proceso de la consola BetterDesk. Todas las operaciones son auditadas.',
            tab_overview: 'Resumen',
            tab_terminal: 'Terminal',
            tab_files: 'Archivos',
            tab_services: 'Servicios',
            memory: 'Memoria',
            load: 'Carga promedio',
            disks: 'Discos',
            history: 'Historial (últimas 60 muestras)',
            host_info: 'Info del host',
            cores: 'núcleos',
            uptime: 'Tiempo activo',
            term_connect: 'Conectar',
            term_disconnect: 'Desconectar',
            term_clear: 'Limpiar',
            term_connecting: 'Conectando…',
            term_connected: 'Conectado',
            term_disconnected: 'Desconectado',
            files_name: 'Nombre',
            files_size: 'Tamaño',
            files_mtime: 'Modificado',
            files_rename: 'Renombrar',
            files_empty: 'Directorio vacío',
            svc_name: 'Servicio',
            svc_state: 'Estado',
            svc_description: 'Descripción',
            svc_start: 'Iniciar',
            svc_stop: 'Detener',
            svc_restart: 'Reiniciar',
            svc_reload: 'Recargar',
            svc_enable: 'Habilitar',
            svc_disable: 'Deshabilitar'
        }
    },
    fr: {
        nav_label: 'Gestion du serveur',
        sm: {
            title: 'Gestion du serveur',
            subtitle: 'Panneau de type Cockpit pour l\'hôte de la console BetterDesk (BÊTA).',
            beta_tooltip: 'Fonction bêta — en développement actif.',
            notice_title: 'Fonction bêta',
            notice_body: 'Ces outils s\'exécutent sur l\'hôte où tourne le processus de la console BetterDesk. Toutes les opérations sont auditées.',
            tab_overview: 'Aperçu',
            tab_terminal: 'Terminal',
            tab_files: 'Fichiers',
            tab_services: 'Services',
            memory: 'Mémoire',
            load: 'Charge moyenne',
            disks: 'Disques',
            history: 'Historique (60 derniers échantillons)',
            host_info: 'Infos hôte',
            cores: 'cœurs',
            uptime: 'Temps d\'activité',
            term_connect: 'Connecter',
            term_disconnect: 'Déconnecter',
            term_clear: 'Effacer',
            term_connecting: 'Connexion…',
            term_connected: 'Connecté',
            term_disconnected: 'Déconnecté',
            files_name: 'Nom',
            files_size: 'Taille',
            files_mtime: 'Modifié',
            files_rename: 'Renommer',
            files_empty: 'Répertoire vide',
            svc_name: 'Service',
            svc_state: 'État',
            svc_description: 'Description',
            svc_start: 'Démarrer',
            svc_stop: 'Arrêter',
            svc_restart: 'Redémarrer',
            svc_reload: 'Recharger',
            svc_enable: 'Activer',
            svc_disable: 'Désactiver'
        }
    },
    it: {
        nav_label: 'Gestione server',
        sm: {
            title: 'Gestione server',
            subtitle: 'Pannello in stile Cockpit per l\'host della console BetterDesk (BETA).',
            tab_overview: 'Panoramica',
            tab_terminal: 'Terminale',
            tab_files: 'File',
            tab_services: 'Servizi',
            memory: 'Memoria',
            load: 'Carico medio',
            disks: 'Dischi',
            uptime: 'Tempo di attività',
            term_connect: 'Connetti',
            term_disconnect: 'Disconnetti',
            term_clear: 'Pulisci',
            term_connected: 'Connesso',
            term_disconnected: 'Disconnesso',
            files_name: 'Nome',
            files_size: 'Dimensione',
            files_mtime: 'Modificato',
            files_rename: 'Rinomina',
            svc_name: 'Servizio',
            svc_state: 'Stato',
            svc_start: 'Avvia',
            svc_stop: 'Ferma',
            svc_restart: 'Riavvia',
            svc_enable: 'Abilita',
            svc_disable: 'Disabilita'
        }
    },
    pt: {
        nav_label: 'Gestão do servidor',
        sm: {
            title: 'Gestão do servidor',
            subtitle: 'Painel tipo Cockpit para o host da console BetterDesk (BETA).',
            tab_overview: 'Visão geral',
            tab_terminal: 'Terminal',
            tab_files: 'Arquivos',
            tab_services: 'Serviços',
            memory: 'Memória',
            disks: 'Discos',
            uptime: 'Tempo ativo',
            term_connect: 'Conectar',
            term_disconnect: 'Desconectar',
            files_name: 'Nome',
            files_size: 'Tamanho',
            files_mtime: 'Modificado',
            svc_name: 'Serviço',
            svc_state: 'Estado',
            svc_start: 'Iniciar',
            svc_stop: 'Parar',
            svc_restart: 'Reiniciar'
        }
    },
    nl: {
        nav_label: 'Serverbeheer',
        sm: {
            title: 'Serverbeheer',
            subtitle: 'Cockpit-achtig paneel voor de BetterDesk-consolehost (BÈTA).',
            tab_overview: 'Overzicht',
            tab_terminal: 'Terminal',
            tab_files: 'Bestanden',
            tab_services: 'Services',
            memory: 'Geheugen',
            disks: 'Schijven',
            uptime: 'Uptime',
            term_connect: 'Verbinden',
            term_disconnect: 'Verbinding verbreken',
            files_name: 'Naam',
            files_size: 'Grootte',
            svc_name: 'Service',
            svc_state: 'Status',
            svc_start: 'Starten',
            svc_stop: 'Stoppen',
            svc_restart: 'Herstarten'
        }
    },
    zh: {
        nav_label: '服务器管理',
        sm: {
            title: '服务器管理',
            subtitle: '类似 Cockpit 的 BetterDesk 控制台主机管理面板（测试版）。',
            beta_tooltip: '测试版功能 — 正在积极开发中。',
            notice_title: '测试版功能',
            notice_body: '这些工具在运行 BetterDesk 控制台进程的主机上执行。所有操作均被审计。',
            tab_overview: '概览',
            tab_terminal: '终端',
            tab_files: '文件',
            tab_services: '服务',
            cpu: 'CPU',
            memory: '内存',
            load: '负载平均值',
            disks: '磁盘',
            history: '历史记录（最近 60 个样本）',
            host_info: '主机信息',
            cores: '核心',
            uptime: '运行时间',
            term_connect: '连接',
            term_disconnect: '断开',
            term_clear: '清除',
            term_connecting: '连接中…',
            term_connected: '已连接',
            term_disconnected: '已断开',
            files_name: '名称',
            files_size: '大小',
            files_perms: '权限',
            files_mtime: '修改时间',
            files_rename: '重命名',
            files_empty: '空目录',
            svc_name: '服务',
            svc_state: '状态',
            svc_description: '说明',
            svc_start: '启动',
            svc_stop: '停止',
            svc_restart: '重启',
            svc_reload: '重新加载',
            svc_enable: '启用',
            svc_disable: '禁用'
        }
    },
    'zh-TW': {
        nav_label: '伺服器管理',
        sm: {
            title: '伺服器管理',
            subtitle: '類似 Cockpit 的 BetterDesk 主控台主機管理面板（測試版）。',
            tab_overview: '概覽',
            tab_terminal: '終端機',
            tab_files: '檔案',
            tab_services: '服務',
            memory: '記憶體',
            disks: '磁碟',
            uptime: '運行時間',
            term_connect: '連線',
            term_disconnect: '中斷連線',
            files_name: '名稱',
            files_size: '大小',
            svc_name: '服務',
            svc_state: '狀態',
            svc_start: '啟動',
            svc_stop: '停止',
            svc_restart: '重新啟動'
        }
    },
    ja: {
        nav_label: 'サーバー管理',
        sm: {
            title: 'サーバー管理',
            subtitle: 'BetterDesk コンソールホスト用の Cockpit 風コントロールパネル（ベータ）。',
            tab_overview: '概要',
            tab_terminal: 'ターミナル',
            tab_files: 'ファイル',
            tab_services: 'サービス',
            memory: 'メモリ',
            disks: 'ディスク',
            uptime: '稼働時間',
            term_connect: '接続',
            term_disconnect: '切断',
            files_name: '名前',
            files_size: 'サイズ',
            svc_name: 'サービス',
            svc_state: '状態',
            svc_start: '開始',
            svc_stop: '停止',
            svc_restart: '再起動'
        }
    },
    ko: {
        nav_label: '서버 관리',
        sm: {
            title: '서버 관리',
            subtitle: 'BetterDesk 콘솔 호스트용 Cockpit 스타일 제어판(베타).',
            tab_overview: '개요',
            tab_terminal: '터미널',
            tab_files: '파일',
            tab_services: '서비스',
            memory: '메모리',
            disks: '디스크',
            uptime: '가동 시간',
            term_connect: '연결',
            term_disconnect: '연결 해제',
            files_name: '이름',
            files_size: '크기',
            svc_name: '서비스',
            svc_state: '상태',
            svc_start: '시작',
            svc_stop: '중지',
            svc_restart: '재시작'
        }
    },
    uk: {
        nav_label: 'Керування сервером',
        sm: {
            title: 'Керування сервером',
            subtitle: 'Панель типу Cockpit для хоста консолі BetterDesk (БЕТА).',
            tab_overview: 'Огляд',
            tab_terminal: 'Термінал',
            tab_files: 'Файли',
            tab_services: 'Служби',
            memory: 'Памʼять',
            disks: 'Диски',
            uptime: 'Час роботи',
            term_connect: 'Зʼєднати',
            term_disconnect: 'Розʼєднати',
            files_name: 'Назва',
            files_size: 'Розмір',
            svc_name: 'Служба',
            svc_state: 'Стан',
            svc_start: 'Запустити',
            svc_stop: 'Зупинити',
            svc_restart: 'Перезапустити'
        }
    },
    cs: {
        nav_label: 'Správa serveru',
        sm: {
            title: 'Správa serveru',
            tab_overview: 'Přehled',
            tab_terminal: 'Terminál',
            tab_files: 'Soubory',
            tab_services: 'Služby',
            memory: 'Paměť',
            disks: 'Disky',
            uptime: 'Doba běhu',
            term_connect: 'Připojit',
            term_disconnect: 'Odpojit',
            svc_name: 'Služba',
            svc_state: 'Stav',
            svc_start: 'Spustit',
            svc_stop: 'Zastavit',
            svc_restart: 'Restartovat'
        }
    },
    sv: {
        nav_label: 'Serverhantering',
        sm: {
            title: 'Serverhantering',
            tab_overview: 'Översikt',
            tab_terminal: 'Terminal',
            tab_files: 'Filer',
            tab_services: 'Tjänster',
            memory: 'Minne',
            disks: 'Diskar',
            uptime: 'Drifttid',
            term_connect: 'Anslut',
            term_disconnect: 'Koppla från',
            svc_start: 'Starta',
            svc_stop: 'Stoppa',
            svc_restart: 'Starta om'
        }
    },
    da: {
        nav_label: 'Serveradministration',
        sm: {
            title: 'Serveradministration',
            tab_overview: 'Oversigt',
            tab_terminal: 'Terminal',
            tab_files: 'Filer',
            tab_services: 'Tjenester',
            memory: 'Hukommelse',
            disks: 'Diske',
            uptime: 'Oppetid',
            term_connect: 'Forbind',
            term_disconnect: 'Afbryd',
            svc_start: 'Start',
            svc_stop: 'Stop',
            svc_restart: 'Genstart'
        }
    },
    nb: {
        nav_label: 'Serveradministrasjon',
        sm: {
            title: 'Serveradministrasjon',
            tab_overview: 'Oversikt',
            tab_terminal: 'Terminal',
            tab_files: 'Filer',
            tab_services: 'Tjenester',
            memory: 'Minne',
            disks: 'Disker',
            uptime: 'Oppetid',
            term_connect: 'Koble til',
            term_disconnect: 'Koble fra'
        }
    },
    fi: {
        nav_label: 'Palvelimen hallinta',
        sm: {
            title: 'Palvelimen hallinta',
            tab_overview: 'Yleiskatsaus',
            tab_terminal: 'Pääte',
            tab_files: 'Tiedostot',
            tab_services: 'Palvelut',
            memory: 'Muisti',
            disks: 'Levyt',
            uptime: 'Käyntiaika',
            term_connect: 'Yhdistä',
            term_disconnect: 'Katkaise'
        }
    },
    hu: {
        nav_label: 'Kiszolgáló-kezelés',
        sm: {
            title: 'Kiszolgáló-kezelés',
            tab_overview: 'Áttekintés',
            tab_terminal: 'Terminál',
            tab_files: 'Fájlok',
            tab_services: 'Szolgáltatások',
            memory: 'Memória',
            disks: 'Lemezek',
            uptime: 'Üzemidő',
            term_connect: 'Csatlakozás',
            term_disconnect: 'Lecsatlakozás'
        }
    },
    ro: {
        nav_label: 'Gestionare server',
        sm: {
            title: 'Gestionare server',
            tab_overview: 'Prezentare',
            tab_terminal: 'Terminal',
            tab_files: 'Fișiere',
            tab_services: 'Servicii',
            memory: 'Memorie',
            disks: 'Discuri',
            uptime: 'Timp de funcționare',
            term_connect: 'Conectează',
            term_disconnect: 'Deconectează'
        }
    },
    tr: {
        nav_label: 'Sunucu Yönetimi',
        sm: {
            title: 'Sunucu Yönetimi',
            tab_overview: 'Genel Bakış',
            tab_terminal: 'Terminal',
            tab_files: 'Dosyalar',
            tab_services: 'Hizmetler',
            memory: 'Bellek',
            disks: 'Diskler',
            uptime: 'Çalışma süresi',
            term_connect: 'Bağlan',
            term_disconnect: 'Bağlantıyı kes',
            svc_start: 'Başlat',
            svc_stop: 'Durdur',
            svc_restart: 'Yeniden başlat'
        }
    },
    ar: {
        nav_label: 'إدارة الخادم',
        sm: {
            title: 'إدارة الخادم',
            tab_overview: 'نظرة عامة',
            tab_terminal: 'الطرفية',
            tab_files: 'الملفات',
            tab_services: 'الخدمات',
            memory: 'الذاكرة',
            disks: 'الأقراص',
            uptime: 'وقت التشغيل',
            term_connect: 'اتصال',
            term_disconnect: 'قطع الاتصال'
        }
    },
    hi: {
        nav_label: 'सर्वर प्रबंधन',
        sm: {
            title: 'सर्वर प्रबंधन',
            tab_overview: 'अवलोकन',
            tab_terminal: 'टर्मिनल',
            tab_files: 'फ़ाइलें',
            tab_services: 'सेवाएं',
            memory: 'मेमोरी',
            disks: 'डिस्क',
            uptime: 'अपटाइम'
        }
    },
    id: {
        nav_label: 'Manajemen Server',
        sm: {
            title: 'Manajemen Server',
            tab_overview: 'Ikhtisar',
            tab_terminal: 'Terminal',
            tab_files: 'Berkas',
            tab_services: 'Layanan',
            memory: 'Memori',
            disks: 'Disk',
            uptime: 'Waktu aktif',
            term_connect: 'Hubungkan',
            term_disconnect: 'Putuskan'
        }
    },
    vi: {
        nav_label: 'Quản lý máy chủ',
        sm: {
            title: 'Quản lý máy chủ',
            tab_overview: 'Tổng quan',
            tab_terminal: 'Cửa sổ dòng lệnh',
            tab_files: 'Tệp',
            tab_services: 'Dịch vụ',
            memory: 'Bộ nhớ',
            disks: 'Ổ đĩa',
            uptime: 'Thời gian hoạt động',
            term_connect: 'Kết nối',
            term_disconnect: 'Ngắt kết nối'
        }
    },
    th: {
        nav_label: 'จัดการเซิร์ฟเวอร์',
        sm: {
            title: 'จัดการเซิร์ฟเวอร์',
            tab_overview: 'ภาพรวม',
            tab_terminal: 'เทอร์มินัล',
            tab_files: 'ไฟล์',
            tab_services: 'บริการ',
            memory: 'หน่วยความจำ',
            disks: 'ดิสก์',
            uptime: 'เวลาทำงาน'
        }
    }
};

function deepMerge(base, override) {
    const out = JSON.parse(JSON.stringify(base));
    Object.keys(override || {}).forEach((k) => {
        out[k] = override[k];
    });
    return out;
}

function buildBlockForLang(code) {
    const tr = TRANSLATIONS[code] || {};
    return {
        nav_label: tr.nav_label || EN.nav_label,
        sm: deepMerge(EN.sm, tr.sm || {})
    };
}

function processLang(filePath) {
    const code = path.basename(filePath, '.json');
    const text = fs.readFileSync(filePath, 'utf8');
    const data = JSON.parse(text);

    const block = buildBlockForLang(code);

    // 1. Add nav.server_management
    data.nav = data.nav || {};
    data.nav.server_management = block.nav_label;

    // 2. Add server_mgmt namespace (do not clobber existing — merge)
    data.server_mgmt = Object.assign({}, data.server_mgmt || {}, block.sm);

    fs.writeFileSync(filePath, JSON.stringify(data, null, 2) + '\n', 'utf8');
    return { code, keys: Object.keys(block.sm).length + 1 };
}

function main() {
    const files = fs.readdirSync(LANG_DIR)
        .filter((f) => f.endsWith('.json'))
        .map((f) => path.join(LANG_DIR, f))
        .sort();

    console.log(`Injecting Server Management i18n keys into ${files.length} language files…`);
    files.forEach((fp) => {
        try {
            const r = processLang(fp);
            console.log(`  ✓ ${r.code.padEnd(8)} +${r.keys} keys`);
        } catch (err) {
            console.error(`  ✗ ${path.basename(fp)} — ${err.message}`);
        }
    });
    console.log('Done.');
}

main();
