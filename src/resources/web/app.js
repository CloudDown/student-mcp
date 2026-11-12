const $=id=>document.getElementById(id);let key=location.hash.slice(1)||sessionStorage.getItem('admin');if(key)sessionStorage.setItem('admin',key);history.replaceState(null,'',location.pathname);
const languages=['fr','es','en'];
let language='en';
try{const saved=localStorage.getItem('student-mcp.language');language=languages.includes(saved)?saved:(navigator.language||'en').split('-')[0]}catch{}
if(!languages.includes(language))language='en';
// English is the source language; only UI copy is translated, never account data or server diagnostics.
const translations={
 'Web':['Web','Web'], 'On this computer':['Sur cet ordinateur','En este ordenador'],
 'Choose an assistant app':['Choisis une application assistante','Elige una aplicación asistente'],
 'Scan again':['Relancer le scan','Volver a buscar'], 'Scanning…':['Recherche…','Buscando…'],
 'Detected':['Détectée','Detectada'], 'Not detected':['Non détectée','No detectada'],
 'Settings saved':['Réglages enregistrés','Ajustes guardados'],
 'Remove setup':['Retirer la configuration','Quitar la configuración'], 'Removing…':['Retrait…','Quitando…'],
 'Remove student-mcp from this app? Your other MCP servers stay unchanged and a backup is created.':['Retirer student-mcp de cette application ? Tes autres serveurs MCP restent inchangés et une sauvegarde est créée.','¿Quitar student-mcp de esta aplicación? Tus otros servidores MCP no cambian y se crea una copia de seguridad.'],
    'Other apps':['Autres applications','Otras aplicaciones'],
 'Reload MCP in the app to use the saved settings.':['Recharge les MCP dans l’application pour utiliser ces réglages.','Recarga los MCP en la aplicación para usar estos ajustes.'],
 'Settings apply automatically in OpenCode.':['Les réglages s’appliquent automatiquement dans OpenCode.','Los ajustes se aplican automáticamente en OpenCode.'],
 'Connect with browser':['Se connecter avec le navigateur','Conectar con el navegador'],
 'Sign in on your school’s website. Only the Moodle mobile token is saved, never your password.':['Connecte-toi sur le site de ton établissement. Seul le jeton mobile Moodle est conservé, jamais ton mot de passe.','Inicia sesión en el sitio de tu institución. Solo se guarda el token móvil de Moodle, nunca tu contraseña.'],
 'Moodle browser sign-in did not finish. Check that your school allows the Moodle mobile app, or choose Manual sign-in to try your password.':['La connexion Moodle n’a pas abouti. Vérifie que ton établissement autorise l’application mobile Moodle, ou choisis la connexion manuelle avec ton mot de passe.','El inicio de sesión de Moodle no finalizó. Comprueba que tu institución permite la aplicación móvil de Moodle o elige el inicio de sesión manual con tu contraseña.'],
 'Manual sign-in':['Connexion manuelle','Inicio de sesión manual'],
 'Rename account':['Renommer le compte','Cambiar el nombre de la cuenta'],
 'Account name':['Nom du compte','Nombre de la cuenta'],
 'e.g. Léa':['Ex. : Léa','P. ej., Léa'],
 'Save':['Enregistrer','Guardar'],
 'Account name saved':['Nom du compte enregistré','Nombre de la cuenta guardado'],
 'Use an account name of at most 80 characters, without control characters.':['Utilise un nom de compte de 80 caractères maximum, sans caractères de contrôle.','Usa un nombre de cuenta de hasta 80 caracteres, sin caracteres de control.'],
 'Session cookie or access token':['Cookie de session ou jeton d’accès','Cookie de sesión o token de acceso'],
 'Sign in on your school’s website. A private browser profile remembers this account; MFA may be requested again.':['Connecte-toi sur le site de ton établissement. Un profil de navigateur privé conserve ce compte ; le MFA peut être redemandé.','Inicia sesión en el sitio de tu institución. Un perfil privado del navegador conserva esta cuenta; se puede volver a solicitar MFA.'],
 'Finish signing in; this page will update automatically.':['Termine la connexion ; cette page se mettra à jour automatiquement.','Termina de iniciar sesión; esta página se actualizará automáticamente.'],
 'Keep Safari or Firefox as your default. If needed, student-mcp downloads a dedicated Chromium-based browser for school sign-in.':['Garde ton navigateur par défaut. Chrome, Edge ou Brave déjà installé est utilisé en priorité. Tout téléchargement nécessite ton accord.','Mantén tu navegador predeterminado. Se usa primero Chrome, Edge o Brave instalado. Cualquier descarga requiere tu permiso.'],
 'No compatible browser found. Download Chrome for Testing from Google (about 200 MB)? Your default browser stays the same.':['Aucun navigateur compatible trouvé. Télécharger Chrome for Testing depuis Google (environ 200 Mo) ? Ton navigateur par défaut reste le même.','No se encontró un navegador compatible. ¿Descargar Chrome for Testing de Google (unos 200 MB)? Tu navegador predeterminado no cambia.'],
 'Download and continue':['Télécharger et continuer','Descargar y continuar'],
 'Not now':['Pas maintenant','Ahora no'],
 'Preparing the sign-in browser…':['Préparation du navigateur de connexion…','Preparando el navegador de inicio de sesión…'],
 'Downloading Chrome for Testing…':['Téléchargement de Chrome for Testing…','Descargando Chrome for Testing…'],
 'Installing the sign-in browser…':['Installation du navigateur de connexion…','Instalando el navegador de inicio de sesión…'],
  'Opening the sign-in window…':['Ouverture de la fenêtre de connexion…','Abriendo la ventana de inicio de sesión…'],
 'Student MCP':['MCP étudiant','MCP estudiante'],
 'Change language':['Changer de langue','Cambiar idioma'], 'Language':['Langue','Idioma'],
 'Install update':['Installer la mise à jour','Instalar actualización'],
 'Checking for updates…':['Vérification des mises à jour…','Buscando actualizaciones…'],
 'Up to date':['À jour','Actualizado'],
 'Update check failed':['Vérification impossible','No se pudo comprobar'],
 'No network connection':['Aucune connexion réseau','Sin conexión de red'],
 'Update failed':['Mise à jour impossible','No se pudo actualizar'],
 'No update available':['Aucune mise à jour disponible','No hay actualización disponible'],
 'Update already in progress':['Mise à jour déjà en cours','Actualización ya en curso'],
 'Downloading update…':['Téléchargement de la mise à jour…','Descargando actualización…'],
 'Update ready. student-mcp will restart or open its installer.':['Mise à jour prête. student-mcp va redémarrer ou ouvrir son installateur.','Actualización lista. student-mcp se reiniciará o abrirá su instalador.'],
 ', selected':[', sélectionné',', seleccionada'],
 'MCP tools':['Outils MCP','Herramientas MCP'],
 'Loading tools…':['Chargement des outils…','Cargando herramientas…'],
 'Could not load tools.':['Impossible de charger les outils.','No se pudieron cargar las herramientas.'],
 'No tools available.':['Aucun outil disponible.','No hay herramientas disponibles.'],
  'Your link':['Ton lien','Tu enlace'], 'Your links':['Tes liens','Tus enlaces'], 'View tools':['Voir les outils','Ver herramientas'], 'Check connection':['Vérifier la connexion','Comprobar conexión'], 'Account information':['Informations du compte','Información de la cuenta'], 'Connection':['Connexion','Conexión'], 'On':['Activé','Activado'], 'Launch at startup':['Lancer au démarrage','Iniciar al arrancar'],
 'Search your school':['Recherche ton établissement','Busca tu institución'], 'No school found':['Aucun établissement trouvé','No se encontró la institución'],
 'Copy':['Copier','Copiar'],
 'Your assistant':['Ton assistant','Tu asistente'],
 'Set up Claude':['Configurer Claude','Configurar Claude'], 'Set up ChatGPT':['Configurer ChatGPT','Configurar ChatGPT'], 'Set up Gemini':['Configurer Gemini','Configurar Gemini'],
 'Set up your assistant':['Configurer ton assistant','Configurar tu asistente'], 'Choose a local app or the web.':['Choisis une application locale ou le web.','Elige una aplicación local o la web.'],
 'Checking local setup…':['Vérification de la configuration locale…','Comprobando la configuración local…'],
 'Restart the assistant to finish setup.':['Redémarre l’assistant pour terminer la configuration.','Reinicia el asistente para terminar la configuración.'],
 'This updates the assistant’s MCP settings. Your other servers stay unchanged and a backup is created.':['Cette action modifie les réglages MCP de l’assistant. Tes autres serveurs restent inchangés et une sauvegarde est créée.','Esto actualiza los ajustes MCP del asistente. Tus otros servidores no cambian y se crea una copia de seguridad.'],
 'Confirm setup':['Confirmer la configuration','Confirmar configuración'], 'Configured':['Configuré','Configurado'],
 'No supported local app was found.':['Aucune application locale compatible n’a été détectée.','No se encontró una aplicación local compatible.'],
 'Gemini CLI not detected':['Gemini CLI non détecté','Gemini CLI no detectado'],
 'Claude Desktop not detected':['Claude Desktop non détecté','Claude Desktop no detectado'],
 'Open Claude Desktop once, then return here, or use the web guide.':['Ouvre Claude Desktop une fois, puis reviens ici, ou utilise le guide web.','Abre Claude Desktop una vez y vuelve aquí, o usa la guía web.'],
 'Install Gemini CLI or use the Gemini web guide.':['Installe Gemini CLI ou utilise le guide web de Gemini.','Instala Gemini CLI o usa la guía web de Gemini.'],
 'ChatGPT connects to remote MCP servers. Your public HTTPS link is required.':['ChatGPT se connecte à des serveurs MCP distants. Ton lien HTTPS public est nécessaire.','ChatGPT se conecta a servidores MCP remotos. Se necesita tu enlace HTTPS público.'],
 'Your public link':['Ton lien public','Tu enlace público'], 'Enable your public link':['Activer ton lien public','Activar tu enlace público'],
 'Your public link is not ready. Open link settings and turn on From the web.':['Ton lien public n’est pas prêt. Ouvre les réglages du lien et active Depuis le web.','Tu enlace público no está listo. Abre los ajustes del enlace y activa Desde la web.'],
 'Open link settings':['Ouvrir les réglages du lien','Abrir ajustes del enlace'], 'Open official settings':['Ouvrir les réglages officiels','Abrir ajustes oficiales'], 'Official guide':['Guide officiel','Guía oficial'],
 'Prepare your link':['Préparer ton lien','Prepara tu enlace'], 'Copy the complete public HTTPS link. Keep it private.':['Copie le lien HTTPS public complet. Garde-le privé.','Copia el enlace HTTPS público completo. Manténlo privado.'],
 'Open Claude connectors':['Ouvrir les connecteurs Claude','Abrir conectores de Claude'], 'Go to Customize > Connectors, select + Add, then Add custom connector.':['Va dans Personnaliser > Connecteurs, choisis + Ajouter, puis Ajouter un connecteur personnalisé.','Ve a Personalizar > Conectores, selecciona + Añadir y luego Añadir conector personalizado.'],
 'Add the MCP server':['Ajouter le serveur MCP','Añadir el servidor MCP'], 'Name it student-mcp and paste your public link. Choose No sign in if Claude asks for authentication.':['Nomme-le student-mcp et colle ton lien public. Choisis Aucune connexion si Claude demande une authentification.','Llámalo student-mcp y pega tu enlace público. Elige Sin inicio de sesión si Claude pide autenticación.'],
 'Enable student-mcp in a chat':['Activer student-mcp dans une conversation','Activar student-mcp en un chat'], 'Use + > Connectors in a conversation and enable student-mcp.':['Utilise + > Connecteurs dans une conversation et active student-mcp.','Usa + > Conectores en una conversación y activa student-mcp.'],
 'Open ChatGPT Plugins':['Ouvrir les plugins ChatGPT','Abrir plugins de ChatGPT'], 'Select +, then Add custom MCP server.':['Choisis +, puis Ajouter un serveur MCP personnalisé.','Selecciona + y luego Añadir servidor MCP personalizado.'],
 'Enter student-mcp as the name and paste the complete HTTPS URL under Connection. Select no authentication when offered.':['Saisis student-mcp comme nom et colle l’URL HTTPS complète dans Connexion. Choisis aucune authentification si l’option est proposée.','Escribe student-mcp como nombre y pega la URL HTTPS completa en Conexión. Selecciona sin autenticación cuando se ofrezca.'],
 'Review and create':['Vérifier et créer','Revisar y crear'], 'Review the risk notice, create the plugin, then check the discovered tools. Availability depends on your plan and workspace permissions.':['Lis l’avertissement, crée le plugin, puis vérifie les outils détectés. La disponibilité dépend de ton forfait et des permissions de l’espace de travail.','Revisa el aviso, crea el plugin y comprueba las herramientas detectadas. La disponibilidad depende de tu plan y de los permisos del espacio de trabajo.'],
 'Use the plugin in a chat':['Utiliser le plugin dans une conversation','Usar el plugin en un chat'], 'Start a chat, type @ and select student-mcp.':['Commence une conversation, saisis @ et choisis student-mcp.','Empieza un chat, escribe @ y selecciona student-mcp.'],
 'Open Gemini Connected Apps':['Ouvrir les applications connectées Gemini','Abrir aplicaciones conectadas de Gemini'], 'Open Settings > Connected Apps, then Add a custom app.':['Ouvre Réglages > Applications connectées, puis Ajouter une application personnalisée.','Abre Ajustes > Aplicaciones conectadas y luego Añadir una aplicación personalizada.'],
 'Paste your public HTTPS MCP URL and select Next. If credentials are requested, review the official authentication instructions.':['Colle ton URL MCP HTTPS publique et choisis Suivant. Si des identifiants sont demandés, suis les instructions officielles d’authentification.','Pega tu URL MCP HTTPS pública y selecciona Siguiente. Si se piden credenciales, sigue las instrucciones oficiales de autenticación.'],
 'Confirm and test':['Confirmer et tester','Confirmar y probar'], 'Follow the security prompts, then use @student-mcp in Gemini. Custom apps currently require an eligible personal Google Account and may be limited by region and language.':['Suis les consignes de sécurité, puis utilise @student-mcp dans Gemini. Les applications personnalisées exigent un compte Google personnel admissible et peuvent être limitées selon la région et la langue.','Sigue los avisos de seguridad y usa @student-mcp en Gemini. Las aplicaciones personalizadas requieren una cuenta personal de Google compatible y pueden variar según la región y el idioma.'],
 'Verify the local connection':['Vérifier la connexion locale','Verificar la conexión local'], 'Restart Claude Desktop, then open + > Connectors to see student-mcp and its tools.':['Redémarre Claude Desktop, puis ouvre + > Connecteurs pour voir student-mcp et ses outils.','Reinicia Claude Desktop y abre + > Conectores para ver student-mcp y sus herramientas.'],
 'Official Claude connector steps':['Étapes officielles des connecteurs Claude','Pasos oficiales de los conectores de Claude'], 'Official ChatGPT plugin steps':['Étapes officielles des plugins ChatGPT','Pasos oficiales de los plugins de ChatGPT'], 'Official Gemini custom app steps':['Étapes officielles des applications Gemini','Pasos oficiales de las aplicaciones de Gemini'],
 'On this computer':['Sur cet ordinateur','En este ordenador'], 'From the web':['Depuis le web','Desde la web'],
 'Active':['Actif','Activo'], 'Off':['Désactivé','Desactivado'], 'Unavailable':['Indisponible','No disponible'],
 'Available to assistants':['Disponible pour les assistants','Disponible para los asistentes'],
 'Click again to disconnect this account.':['Clique encore pour déconnecter ce compte.','Haz clic otra vez para desconectar esta cuenta.'],
 'Creating your public link…':['Création de ton lien public…','Creando tu enlace público…'],
 'Reconnecting your public link…':['Reconnexion de ton lien public…','Reconectando tu enlace público…'],
 'Public link unavailable — click to retry':['Lien public indisponible — réessayer','Enlace público no disponible — reintentar'],
 'Public link off — click to enable':['Lien public désactivé — activer','Enlace público desactivado — activar'],
 'Connect a service to create your link':['Connecte un service pour créer ton lien','Conecta un servicio para crear tu enlace'],
 'Copy link':['Copier le lien','Copiar enlace'], 'Back to services':['Retour aux services','Volver a los servicios'],
 'Back to Moodle':['Retour à Moodle','Volver a Moodle'], 'Back to Omnivox':['Retour à Omnivox','Volver a Omnivox'], 'Back to Brightspace':['Retour à Brightspace','Volver a Brightspace'], 'Back to Canvas':['Retour à Canvas','Volver a Canvas'],
 'Accounts':['Comptes','Cuentas'], 'Add account':['Ajouter un compte','Añadir cuenta'],
 'DevTools → Application/Storage → Cookies':['Outils de développement → Application/Stockage → Cookies','Herramientas de desarrollo → Aplicación/Almacenamiento → Cookies'],
 'Username':['Nom d’utilisateur','Nombre de usuario'], 'firstname.lastname':['prenom.nom','nombre.apellido'],
 'Password':['Mot de passe','Contraseña'], 'Moodle password':['Mot de passe Moodle','Contraseña de Moodle'],
 'Omnivox address':['Adresse Omnivox','Dirección de Omnivox'], 'Student number':['Numéro étudiant','Número de estudiante'],
 'Omnivox password':['Mot de passe Omnivox','Contraseña de Omnivox'], 'Site address':['Adresse du site','Dirección del sitio'],
 'Cookie value':['Valeur du cookie','Valor de la cookie'], 'Checking the connection…':['Vérification de la connexion…','Comprobando la conexión…'],
 'Connecting…':['Connexion…','Conectando…'], 'Save changes':['Enregistrer','Guardar cambios'],
 'Connection failed':['Échec de la connexion','Error de conexión'], 'Checking connection…':['Vérification de la connexion…','Comprobando la conexión…'],
 'Connected':['Connecté','Conectado'], 'Retry':['Réessayer','Reintentar'], 'Edit sign-in':['Modifier les identifiants','Cambiar credenciales'],
 'Disconnect':['Déconnecter','Desconectar'], 'Identity verification':['Vérification d’identité','Verificación de identidad'],
 'Security code (6 digits)':['Code de sécurité (6 chiffres)','Código de seguridad (6 dígitos)'],
 'Verifying…':['Vérification…','Verificando…'], 'Verify and connect':['Vérifier et connecter','Verificar y conectar'],
 'Send a new code':['Envoyer un nouveau code','Enviar un nuevo código'], 'Cancel verification':['Annuler la vérification','Cancelar verificación'],
 'Public link is off. Turn the switch on to restore your stable HTTPS link through srv.us. No account needed.':['Le lien public est désactivé. Active-le pour rétablir ton lien HTTPS stable via srv.us. Aucun compte requis.','El enlace público está desactivado. Actívalo para restablecer tu enlace HTTPS estable mediante srv.us. No necesitas una cuenta.'],
 'Opening your stable HTTPS link through srv.us.':['Ouverture de ton lien HTTPS stable via srv.us.','Abriendo tu enlace HTTPS estable mediante srv.us.'],
 'Reconnecting to srv.us. Your saved link stays the same.':['Reconnexion à srv.us. Ton lien enregistré reste identique.','Reconectando a srv.us. Tu enlace guardado sigue siendo el mismo.'],
 'The public link was not created. Turn the switch on to try again.':['Le lien public n’a pas été créé. Active-le pour réessayer.','No se ha creado el enlace público. Actívalo para volver a intentarlo.'],
 'No service is connected yet. Turn the switch on to create the HTTPS link anyway.':['Aucun service n’est encore connecté. Active-le pour créer quand même le lien HTTPS.','Todavía no hay servicios conectados. Activa el enlace para crear el enlace HTTPS de todos modos.'],
 'Copied':['Copié','Copiado'], 'Request failed':['Échec de la requête','Error en la solicitud'],
 'Disconnected':['Déconnecté','Desconectado'], 'Code sent':['Code envoyé','Código enviado'], 'Cancelled':['Annulé','Cancelado'],
 'student-mcp stopped':['student-mcp arrêté','student-mcp detenido'],
 'No pending verification':['Aucune vérification en cours','No hay ninguna verificación pendiente'],
 'Omnivox is not connected anymore.':['Omnivox n’est plus connecté.','Omnivox ya no está conectado.'],
 'Brightspace is not connected anymore.':['Brightspace n’est plus connecté.','Brightspace ya no está conectado.'],
 'Moodle is not connected anymore.':['Moodle n’est plus connecté.','Moodle ya no está conectado.'],
 'Enter the 6-digit security code sent to your Omnivox Mobile app.':['Saisis le code de sécurité à 6 chiffres envoyé à l’application Omnivox Mobile.','Introduce el código de seguridad de 6 dígitos enviado a la aplicación Omnivox Mobile.'],
 'Enter the 6-digit code from your authenticator or security token.':['Saisis le code à 6 chiffres de ton application d’authentification ou de ton jeton de sécurité.','Introduce el código de 6 dígitos de tu aplicación de autenticación o token de seguridad.'],
 'Enter the 6-digit security code provided by your school.':['Saisis le code de sécurité à 6 chiffres fourni par ton établissement.','Introduce el código de seguridad de 6 dígitos proporcionado por tu institución.'],
 'Enter the 6-digit security code from Omnivox.':['Saisis le code de sécurité Omnivox à 6 chiffres.','Introduce el código de seguridad de Omnivox de 6 dígitos.'],
 'Omnivox blocked further code attempts. Wait a few minutes, then sign in again.':['Omnivox a bloqué les nouvelles tentatives. Attends quelques minutes, puis reconnecte-toi.','Omnivox bloqueó nuevos intentos. Espera unos minutos y vuelve a iniciar sesión.'],
 'The security code is invalid or has expired. Try the latest code or request a new one.':['Le code de sécurité est invalide ou expiré. Essaie le plus récent ou demandes-en un nouveau.','El código de seguridad no es válido o ha caducado. Prueba el más reciente o solicita uno nuevo.'],
 'A new code cannot be requested for this verification session.':['Impossible de demander un nouveau code pour cette session de vérification.','No se puede solicitar un nuevo código para esta sesión de verificación.'],
 'Omnivox has reached its code resend limit. Wait a few minutes before requesting another code.':['La limite de renvoi de codes Omnivox est atteinte. Attends quelques minutes avant de réessayer.','Se alcanzó el límite de reenvío de códigos de Omnivox. Espera unos minutos antes de intentarlo de nuevo.'],
 'Omnivox did not confirm sending a new code. Try again.':['Omnivox n’a pas confirmé l’envoi d’un nouveau code. Réessaie.','Omnivox no confirmó el envío de un nuevo código. Inténtalo de nuevo.'],
 'This Omnivox identity verification method is not supported by the app.':['Cette méthode de vérification Omnivox n’est pas prise en charge.','Esta aplicación no admite este método de verificación de Omnivox.'],
 'Connection status unavailable. Retrying…':['État de connexion indisponible. Nouvelle tentative…','Estado de conexión no disponible. Reintentando…'],
 'Could not copy the link. Open link settings and select the link to copy it manually.':['Impossible de copier le lien. Ouvre les réglages du lien et sélectionne le lien pour le copier manuellement.','No se pudo copiar el enlace. Abre los ajustes del enlace y selecciona el enlace para copiarlo manualmente.']
};
// Localized UI summaries keyed by stable MCP tool name. MCP names and schemas remain untouched.
// Keep a summary for every tool returned by /tools/{service}; the browser test checks coverage.
Object.assign(translations,{
 'Keep this link private.':['Garde ce lien privé.','Mantén este enlace privado.'],
 'Click path':['Boutons à suivre','Botones que seguir'],
 'Free: 1 custom connector.':['Free : 1 connecteur personnalisé.','Free: 1 conector personalizado.'],
 'Your custom app':['Ton application personnalisée','Tu aplicación personalizada'],
 'Use student-mcp: what are my deadlines this week?':['Avec student-mcp, quelles sont mes échéances cette semaine ?','Con student-mcp, ¿cuáles son mis fechas de entrega esta semana?'],
 'Paste the complete HTTPS link in the connector settings, not in a conversation. Keep student-mcp running.':['Colle le lien HTTPS complet dans les réglages du connecteur, pas dans une conversation. Garde student-mcp ouvert.','Pega el enlace HTTPS completo en los ajustes del conector, no en una conversación. Mantén student-mcp abierto.'],
 'Close':['Fermer','Cerrar'],
 'Cancel':['Annuler','Cancelar'],
 'Availability depends on your plan and workspace permissions.':['La disponibilité dépend de ton forfait et des permissions de ton espace de travail.','La disponibilidad depende de tu plan y de los permisos de tu espacio de trabajo.'],
 'Set up an assistant':['Configurer un assistant','Configurar un asistente'],
 'Your courses, one conversation away.':['Tes cours, directement dans ton assistant.','Tus cursos, directamente en tu asistente.'],
 'Set up Grok':['Configurer Grok','Configurar Grok'],
 'Open Grok settings':['Ouvrir Grok','Abrir Grok'],
 'Screenshot arrows':['Flèches de la capture','Flechas de la captura'],
 'Open Grok’s Plugins page, then click New Connector.':['Ouvre la page Plugins de Grok, puis clique sur New Connector.','Abre la página Plugins de Grok y haz clic en New Connector.'],
 'In the New Connector dialog, select Custom.':['Dans la fenêtre New Connector, sélectionne Custom.','En la ventana New Connector, selecciona Custom.'],
 'Enter student-mcp under Name and paste your complete link under Server URL, then click Add Connector.':['Saisis student-mcp dans Name et colle ton lien complet dans Server URL, puis clique sur Add Connector.','Escribe student-mcp en Name y pega tu enlace completo en Server URL; después haz clic en Add Connector.'],
 'Grok’s placeholder shows /sse. Keep your complete /mcp/ link unchanged. The connector was not submitted during capture.':['L’exemple de Grok affiche /sse. Garde ton lien /mcp/ complet sans le modifier. Le connecteur n’a pas été enregistré lors de la capture.','El ejemplo de Grok muestra /sse. Conserva tu enlace /mcp/ completo sin modificarlo. No se guardó el conector durante la captura.'],
 'Start a new conversation and ask Grok to use student-mcp.':['Ouvre une conversation et demande à Grok d’utiliser student-mcp.','Abre una conversación y pide a Grok que use student-mcp.'],
 'Connection method':['Mode de connexion','Método de conexión'],
 'Setup steps':['Étapes de connexion','Pasos de conexión'],
 'Open settings':['Ouvrir les réglages','Abrir ajustes'],
 'Add the server':['Ajouter le serveur','Añadir el servidor'],
 'Ask a question':['Poser une question','Hacer una pregunta'],
 'Settings':['Réglages','Ajustes'], 'Connect':['Connexion','Conectar'], 'Try it':['Essai','Probar'],
 'Check the app':['Vérifier l’application','Comprobar la aplicación'], 'Set up':['Configurer','Configurar'], 'Restart':['Redémarrer','Reiniciar'],
 'App':['Appli','App'], 'Setup':['Réglages','Ajustes'],
 'This link lets your assistants access the school accounts you have enabled.':['Ce lien permet à tes assistants d’accéder aux comptes scolaires que tu as activés.','Este enlace permite que tus asistentes accedan a las cuentas escolares que has activado.'],
 'Keep this link private. Paste it in the connector settings, never in a conversation.':['Garde ce lien privé. Colle-le dans les réglages du connecteur, jamais dans une conversation.','Mantén este enlace privado. Pégalo en los ajustes del conector, nunca en una conversación.'],
 'Keep student-mcp running and this computer awake while using your assistant.':['Garde student-mcp ouvert et cet ordinateur allumé pendant que tu utilises ton assistant.','Mantén student-mcp abierto y este ordenador encendido mientras usas tu asistente.'],
 'Your school':['Ton établissement','Tu institución'],
 'Your school connects through student-mcp to your assistant':['Ton établissement se connecte à ton assistant via student-mcp','Tu institución se conecta a tu asistente a través de student-mcp'],
 'Open the connector settings':['Ouvre les réglages MCP','Abre los ajustes MCP'],
 'Add a connection in your assistant, not in the chat.':['Ajoute une connexion dans ton assistant, pas dans la conversation.','Añade una conexión en tu asistente, no en el chat.'],
 'Open the settings in a new tab, then follow these clicks.':['Ouvre les réglages dans un nouvel onglet, puis suis ces étapes.','Abre los ajustes en otra pestaña y sigue estos pasos.'],
 'Open your assistant’s settings, then find MCP, Connectors or Integrations.':['Dans les réglages de ton assistant, cherche MCP, Connecteurs ou Intégrations.','En los ajustes de tu asistente, busca MCP, Conectores o Integraciones.'],
 'Choose Add server or Add custom connector. The exact labels depend on your app.':['Choisis Ajouter un serveur ou Ajouter un connecteur personnalisé. Le libellé dépend de ton application.','Elige Añadir servidor o Añadir conector personalizado. El nombre depende de tu aplicación.'],
 'Open Customize → Connectors.':['Ouvre Personnalisation → Connecteurs.','Abre Personalizar → Conectores.'],
 'Click + Add → Add custom connector.':['Clique sur + Ajouter → Ajouter un connecteur personnalisé.','Haz clic en + Añadir → Añadir conector personalizado.'],
 'Open ChatGPT Plugins.':['Ouvre les Plugins de ChatGPT.','Abre los Plugins de ChatGPT.'],
 'Click + → Add custom MCP server.':['Clique sur + → Add custom MCP server.','Haz clic en + → Add custom MCP server.'],
 'Open Settings → Connected Apps.':['Ouvre Settings → Connected Apps.','Abre Settings → Connected Apps.'],
 'Click + Custom at the top right.':['Clique sur + Custom en haut à droite.','Haz clic en + Custom arriba a la derecha.'],
 'Open Claude settings':['Ouvrir Claude','Abrir Claude'], 'Open ChatGPT settings':['Ouvrir ChatGPT','Abrir ChatGPT'], 'Open Gemini settings':['Ouvrir Gemini','Abrir Gemini'],
 'Claude · Add custom connector':['Claude · Ajouter un connecteur personnalisé','Claude · Añadir conector personalizado'],
 'Gemini · Connected Apps → + Custom':['Gemini · Connected Apps → + Custom','Gemini · Connected Apps → + Custom'],
 'Gemini · MCP Server URL → Next':['Gemini · MCP Server URL → Next','Gemini · MCP Server URL → Next'],
 'OpenAI · documented setup steps':['OpenAI · étapes du guide officiel','OpenAI · pasos de la guía oficial'],
 'Helium capture':['Capture Helium','Captura de Helium'], 'Screenshot':['Capture d’écran','Captura de pantalla'], 'Enlarge screenshot':['Agrandir la capture','Ampliar captura'],
 'Free accounts allow one custom connector. If Add is disabled, check your plan’s limit.':['Le forfait Free autorise un connecteur personnalisé. Si Ajouter est grisé, vérifie la limite de ton forfait.','El plan Free permite un conector personalizado. Si Añadir está desactivado, comprueba el límite de tu plan.'],
 'Availability depends on your plan and workspace permissions. These steps follow OpenAI’s guide; the signed-in form could not be verified in Helium.':['La disponibilité dépend de ton forfait et de ton espace de travail. Ces étapes suivent le guide OpenAI ; le formulaire connecté n’a pas pu être vérifié dans Helium.','La disponibilidad depende de tu plan y espacio de trabajo. Estos pasos siguen la guía de OpenAI; no se pudo verificar el formulario conectado en Helium.'],
 'Google currently lists these requirements: personal account, age 18+, US, English and Keep Activity on. Availability may change.':['Google indique actuellement : compte personnel, 18 ans et plus, États-Unis, anglais et activité conservée. La disponibilité peut évoluer.','Google indica actualmente: cuenta personal, 18 años o más, Estados Unidos, inglés y actividad guardada. La disponibilidad puede cambiar.'],
 'Button names can vary with your account’s language and plan.':['Les noms des boutons peuvent varier selon la langue et le forfait de ton compte.','Los nombres de los botones pueden variar según el idioma y el plan de tu cuenta.'],
 'Add student-mcp':['Ajoute student-mcp','Añade student-mcp'],
 'Fill in these values in the assistant’s form.':['Renseigne ces valeurs dans le formulaire de l’assistant.','Introduce estos valores en el formulario del asistente.'],
 'The MCP Server URL field takes the complete HTTPS link, including the /mcp/ path.':['Le champ MCP Server URL attend le lien HTTPS complet, avec le chemin /mcp/.','El campo MCP Server URL requiere el enlace HTTPS completo, incluida la ruta /mcp/.'],
 'Name':['Nom','Nombre'], 'Transport':['Transport','Transporte'], 'Server URL':['URL du serveur','URL del servidor'], 'Remote MCP server URL':['URL du serveur MCP distant','URL del servidor MCP remoto'], 'Authentication':['Authentification','Autenticación'], 'Description':['Description','Descripción'], 'Connection':['Connection','Connection'],
 'None':['Aucune','Ninguna'], 'None, if offered':['Aucune, si proposé','Ninguna, si se ofrece'],
 'Your complete public HTTPS link':['Ton lien HTTPS public complet','Tu enlace HTTPS público completo'],
 'My courses, deadlines and school files':['Mes cours, échéances et fichiers scolaires','Mis cursos, fechas de entrega y archivos escolares'],
 'Save the server, then allow the tools you want to use. Do not enter your school password.':['Enregistre le serveur, puis autorise les outils souhaités. Ne saisis pas ton mot de passe scolaire.','Guarda el servidor y autoriza las herramientas que quieras usar. No introduzcas tu contraseña escolar.'],
  'Click Continue, review the detected authentication, then Continue → No sign in → Add. These form steps follow Anthropic’s guide; only the Add menu was verified in Helium.':['Clique sur Continuer (« Continue »), vérifie l’authentification détectée, puis Continuer → Aucune connexion (« No sign in ») → Ajouter (« Add »). Ces étapes suivent le guide Anthropic ; seul le menu Ajouter a été vérifié dans Helium.','Haz clic en Continuar (« Continue »), revisa la autenticación detectada y luego Continuar → Sin inicio de sesión (« No sign in ») → Añadir (« Add »). Estos pasos siguen la guía de Anthropic; solo se verificó el menú Añadir en Helium.'],
  'No sign in':['Aucune connexion','Sin inicio de sesión'],
  'No authentication':['Aucune authentification','Sin autenticación'],
  'Add':['Ajouter','Añadir'],
  'Connectors':['Connecteurs','Conectores'],
  'Claude · Add → Add custom connector':['Claude · Ajouter → Ajouter un connecteur personnalisé','Claude · Añadir → Añadir conector personalizado'],
 'Review the risk notice → I understand and want to continue → Create as a plugin. Check the tools discovered by ChatGPT.':['Lis l’avertissement → I understand and want to continue → Create as a plugin. Vérifie les outils détectés par ChatGPT.','Lee el aviso → I understand and want to continue → Create as a plugin. Comprueba las herramientas detectadas por ChatGPT.'],
 'Paste your complete link in MCP Server URL.':['Colle ton lien complet dans MCP Server URL.','Pega tu enlace completo en MCP Server URL.'],
 'Click Next, then review the security prompts.':['Clique sur Next, puis lis les consignes de sécurité.','Haz clic en Next y revisa los avisos de seguridad.'],
 'No school password is needed. If Gemini requests OAuth credentials, do not invent any: check the official guide. Screens after Next were not verified.':['Aucun mot de passe scolaire n’est nécessaire. Si Gemini demande des identifiants OAuth, n’en invente pas : consulte le guide officiel. Les écrans après Next n’ont pas été vérifiés.','No se necesita la contraseña escolar. Si Gemini solicita credenciales OAuth, no las inventes: consulta la guía oficial. No se verificaron las pantallas posteriores a Next.'],
 'The secret is already in your link. student-mcp does not provide an OAuth client ID or client secret.':['Le secret est déjà dans ton lien. student-mcp ne fournit pas de client ID ni de client secret OAuth.','El secreto ya está en tu enlace. student-mcp no proporciona un client ID ni un client secret de OAuth.'],
 'Ask your first question':['Pose ta première question','Haz tu primera pregunta'],
 'What are my deadlines this week?':['Quelles sont mes échéances cette semaine ?','¿Cuáles son mis fechas de entrega esta semana?'],
 'Enable student-mcp’s tools in a new conversation.':['Active les outils de student-mcp dans une nouvelle conversation.','Activa las herramientas de student-mcp en una nueva conversación.'],
 'In a conversation, click + → Connectors and enable student-mcp.':['Dans une conversation, clique sur + → Connecteurs et active student-mcp.','En una conversación, haz clic en + → Conectores y activa student-mcp.'],
 'Start a new conversation, type @ and select student-mcp.':['Ouvre une conversation, saisis @ et sélectionne student-mcp.','Abre una conversación, escribe @ y selecciona student-mcp.'],
 'After connecting, type @ in a conversation and select your custom app.':['Après la connexion, saisis @ dans une conversation et sélectionne ton application personnalisée.','Una vez conectado, escribe @ en una conversación y selecciona tu aplicación personalizada.'],
 'Your assistant should use student-mcp to check your courses. Approve only the tools and actions you recognise.':['Ton assistant devrait utiliser student-mcp pour consulter tes cours. N’autorise que les outils et actions que tu reconnais.','Tu asistente debería usar student-mcp para consultar tus cursos. Autoriza solo las herramientas y acciones que reconozcas.'],
 'If no tools appear, check that a school account is connected and enabled, and that student-mcp is still running.':['Si aucun outil n’apparaît, vérifie qu’un compte scolaire est connecté et activé, et que student-mcp fonctionne encore.','Si no aparece ninguna herramienta, comprueba que una cuenta escolar esté conectada y activada y que student-mcp siga funcionando.'],
 'Details and help':['Détails et aide','Detalles y ayuda'], 'Previous':['Précédent','Anterior'], 'Continue':['Continuer','Continuar'], 'Finish guide':['Terminer le guide','Terminar la guía'],
 'Set up a local assistant':['Configurer un assistant local','Configurar un asistente local'],
 'Use stdio when your assistant runs on this computer. No public link is needed.':['Utilise stdio si ton assistant fonctionne sur cet ordinateur. Aucun lien public n’est nécessaire.','Usa stdio si tu asistente funciona en este ordenador. No se necesita un enlace público.'],
 'Command':['Commande','Comando'], 'Arguments':['Arguments','Argumentos'],
 'Use the installed student-mcp executable. If it is not on PATH, enter its full path.':['Utilise l’exécutable student-mcp installé. S’il n’est pas dans le PATH, indique son chemin complet.','Usa el ejecutable student-mcp instalado. Si no está en PATH, introduce su ruta completa.'],
 'Choose Add server or Add custom connector.':['Choisis Ajouter un serveur ou Ajouter un connecteur personnalisé.','Elige Añadir servidor o Añadir conector personalizado.'],
 'Select stdio or Local command, not a remote URL.':['Choisis stdio ou Commande locale, pas une URL distante.','Elige stdio o Comando local, no una URL remota.'],
 'Do not pass a school password, session token or public link as an argument.':['Ne passe ni mot de passe scolaire, ni jeton de session, ni lien public en argument.','No pases contraseñas escolares, tokens de sesión ni enlaces públicos como argumentos.'],
 'Connect on this computer':['Connecte cet ordinateur','Conecta este ordenador'],
 'Set up the local connection':['Configure la connexion locale','Configura la conexión local'],
 'Claude Desktop can use student-mcp directly, without a public link.':['Claude Desktop peut utiliser student-mcp directement, sans lien public.','Claude Desktop puede usar student-mcp directamente, sin enlace público.'],
 'Gemini CLI can use student-mcp directly. This does not configure the Gemini website.':['Gemini CLI peut utiliser student-mcp directement. Cela ne configure pas le site Gemini.','Gemini CLI puede usar student-mcp directamente. Esto no configura el sitio web de Gemini.'],
 'Only Confirm setup changes a file. Other MCP servers are preserved and a backup is created.':['Seul Confirmer la configuration modifie un fichier. Les autres serveurs MCP sont conservés et une sauvegarde est créée.','Solo Confirmar configuración modifica un archivo. Los otros servidores MCP se conservan y se crea una copia de seguridad.'],
 'Restart your assistant':['Redémarre ton assistant','Reinicia tu asistente'],
 'Quit Claude Desktop completely and reopen it.':['Quitte complètement Claude Desktop, puis rouvre-le.','Cierra Claude Desktop por completo y vuelve a abrirlo.'],
 'Close and restart Gemini CLI.':['Ferme et relance Gemini CLI.','Cierra y reinicia Gemini CLI.'],
 'Run /mcp in Gemini CLI and check that student-mcp appears.':['Saisis /mcp dans Gemini CLI et vérifie que student-mcp apparaît.','Escribe /mcp en Gemini CLI y comprueba que aparezca student-mcp.'],
 'Approve only the tools and actions you recognise.':['N’autorise que les outils et actions que tu reconnais.','Autoriza solo las herramientas y acciones que reconozcas.']
});
const toolDescriptions={
 moodle_status:['Vérifie la connexion Moodle et indique le site et le compte associés au jeton.','Comprueba la conexión a Moodle e indica el sitio y la cuenta asociados al token.'],
 moodle_my_courses:['Liste les cours inscrits avec leurs identifiants, noms et dates. Les cours masqués ou terminés sont exclus sauf si include_hidden est activé.','Enumera los cursos inscritos con identificadores, nombres y fechas. Excluye los cursos ocultos o finalizados salvo que se active include_hidden.'],
 moodle_search:['Recherche les noms, sections et consignes via un index local (15 minutes). Le contenu des PDF n’est pas recherché et aucun cache texte PDF n’est consulté : lisez d’abord le PDF avec moodle_file. Suivez continuation ; vérifiez partial/errors.','Busca nombres, secciones e instrucciones con un índice local (15 minutos). El contenido de los PDF no se busca y no se consulta ninguna caché de texto PDF: lee primero el PDF con moodle_file. Sigue continuation; revisa partial/errors.'],
 moodle_course_content:['Vue bornée par pages (6 sections), sous-sections incluses, URL directe et dates. group_only filtre selon les restrictions de groupe Moodle (uservisible), sans deviner d’après les noms. sectionid sélectionne une section. detail=full donne les consignes complètes et accepte query pour ne renvoyer que les éléments trouvés ; suivez nextPage.','Vista limitada por páginas (6 secciones), incluidas subsecciones, URL directa y fechas. group_only filtra según las restricciones de grupo de Moodle (uservisible), sin adivinar por nombres. sectionid selecciona una sección. detail=full da las instrucciones completas y acepta query para devolver solo los elementos encontrados; sigue nextPage.'],
 moodle_resources:['Liste les pages, fichiers, liens, dossiers et livres avec leur section Moodle (id et nom). Pour l’arbre complet d’une semaine, utilisez moodle_course_content.','Enumera páginas, archivos, enlaces, carpetas y libros con su sección de Moodle (id y nombre). Para el árbol completo de una semana, usa moodle_course_content.'],
 moodle_file:['Télécharge l’original via fileurl. Pour étudier, text ou images envoie le texte et chaque page en image (4 pages par défaut, 8 maximum). Office est converti localement en PDF dans un environnement isolé, sinon renvoyé en blocs texte avec limites explicites. Les tableurs restent des cellules. Réutilisez documentId comme document_id pour le même compte pendant 5 minutes. find recherche le texte ; suivez nextPageFrom. Les jetons restent privés.','Descarga el original mediante fileurl. Para estudiar, text o images envía texto y una imagen de cada página (4 por defecto, 8 máximo). Office se convierte localmente a PDF de forma aislada; si no está disponible, devuelve bloques de texto con limitaciones explícitas. Las hojas de cálculo conservan las celdas. Reutiliza documentId como document_id para la misma cuenta durante 5 minutos. find busca texto; sigue nextPageFrom. Los tokens siguen privados.'],
 moodle_assignments:['Résume les devoirs (titre, cours, date limite, statut) et leur submissionStatus. Par défaut, statut seulement pour les devoirs ouverts ou dus sous 14 jours, avec cache 3 min ; full_status=true pour tout. Consultez statusesMissing.','Resume las tareas (título, curso, fecha límite, estado) y su submissionStatus. Por defecto, solo tareas abiertas o con vencimiento en 14 días, con caché de 3 min; full_status=true para todas. Revisa statusesMissing.'],
 moodle_assignment_status:['Affiche un état cohérent du rendu et de la notation : statut, date de remise, canSubmit et sa raison, fichiers déposés. assignid est l’id du devoir ; un cmid est aussi accepté et résolu.','Muestra un estado coherente de entrega y calificación: estado, fecha de envío, canSubmit y su motivo, archivos enviados. assignid es el id de la tarea; también se acepta y resuelve un cmid.'],
 moodle_quizzes:['Liste les quiz visibles, avec filtre facultatif par identifiants de cours.','Enumera los cuestionarios visibles, con un filtro opcional por identificadores de curso.'],
 moodle_quiz_status:['Affiche les tentatives, la meilleure note et les conditions d’accès d’un quiz.','Muestra los intentos, la mejor nota y las condiciones de acceso de un cuestionario.'],
 moodle_grades:['Affiche les notes et les vrais poids Moodle (éléments ou rapport utilisateur), avec le total Moodle. Si aucun poids n’est fourni, weightsAvailable=false et la réponse le dit ; aucun poids n’est inventé.','Muestra las notas y los pesos reales de Moodle (elementos o informe de usuario), con el total de Moodle. Si no hay pesos, weightsAvailable=false y la respuesta lo indica; no se inventa ninguno.'],
 moodle_calendar:['Affiche le calendrier Moodle à venir, avec filtre facultatif par cours.','Muestra el calendario de Moodle próximo, con un filtro opcional por curso.'],
 moodle_deadlines:['Rassemble les échéances des devoirs, quiz et événements Moodle, triées par date. include_expired inclut aussi les éléments déjà échus dans la fenêtre.','Reúne las fechas límite de tareas, cuestionarios y eventos de Moodle, ordenadas por fecha. include_expired incluye también los elementos ya vencidos en la ventana.'],
 moodle_announcements:['Affiche les annonces récentes des cours, en texte brut, des plus récentes aux plus anciennes.','Muestra los anuncios recientes de los cursos en texto plano, del más reciente al más antiguo.'],
 moodle_forums:['Liste les forums des cours, y compris les annonces et les discussions.','Enumera los foros de los cursos, incluidos los de anuncios y debate.'],
 moodle_forum_discussions:['Liste les discussions récentes d’un forum.','Enumera los debates recientes de un foro.'],
 moodle_forum_posts:['Lit les messages d’une discussion à l’aide de discussionid.','Lee las publicaciones de un debate mediante discussionid.'],
 moodle_notifications:['Affiche le nombre de notifications non lues et les notifications Moodle récentes.','Muestra el número de notificaciones sin leer y las notificaciones recientes de Moodle.'],
 moodle_messages:['Affiche les conversations privées et le nombre de messages non lus.','Muestra las conversaciones privadas y el número de mensajes sin leer.'],
 moodle_conversation_messages:['Lit les messages d’une conversation privée.','Lee los mensajes de una conversación privada.'],
 moodle_completion:['Affiche l’état d’achèvement des activités d’un cours.','Muestra el estado de finalización de las actividades de un curso.'],
 ws_discover:['Recherche les fonctions Web Service Moodle accessibles au jeton, avec leur description et leur type d’accès.','Busca las funciones de servicio web de Moodle accesibles para el token, con su descripción y tipo de acceso.'],
 ws_schema:['Affiche les paramètres et le type lecture/écriture d’une fonction du catalogue Moodle. Ne prouve pas que le jeton peut l’appeler : vérifiez avec ws_discover ou site_info.','Muestra los parámetros y el tipo de lectura/escritura de una función del catálogo de Moodle. No demuestra que el token pueda usarla: compruébalo con ws_discover o site_info.'],
 ws_call:['Appelle une fonction Web Service Moodle par son nom exact. Privilégiez les outils moodle_*, puis cherchez avec ws_discover. Peut modifier les données scolaires : demandez confirmation avant toute écriture. Moodle applique aussi les permissions du jeton.','Llama a una función de servicio web de Moodle por su nombre exacto. Usa primero las herramientas moodle_* y busca con ws_discover. Puede modificar datos académicos: pide confirmación antes de escribir. Moodle también aplica los permisos del token.'],
 moodle_recent_grades:['Dernières notes d’évaluations datées des cours actuels, par ordre chronologique inverse. Exclut les notes sans date et indique les erreurs par cours ; les dates correspondent à la correction, pas à la publication.','Últimas calificaciones de evaluaciones con fecha en cursos actuales, de la más reciente a la más antigua. Excluye notas sin fecha e indica errores por curso; las fechas son de corrección, no de publicación.'],
 moodle_course_updates:['Modifications des activités d’un cours depuis un horodatage Unix : fichiers, réglages, notes et forums signalés par Moodle. Ne constitue pas un journal complet.','Cambios en las actividades de un curso desde una marca de tiempo Unix: archivos, ajustes, notas y foros registrados por Moodle. No es un registro completo.'],
 omnivox_status:['Vérifie le compte Omnivox / Léa connecté.','Comprueba la cuenta conectada de Omnivox / Léa.'],
 omnivox_my_courses:['Liste les cours Omnivox / Léa du trimestre actuel. Les valeurs course_id sont des chaînes, pas des nombres.','Enumera los cursos de Omnivox / Léa del período actual. Los valores de course_id son cadenas, no números.'],
 omnivox_schedule:['Affiche l’horaire hebdomadaire statique de l’étudiant.','Muestra el horario semanal estático del estudiante.'],
 omnivox_course_documents:['Liste les métadonnées des documents Léa d’un cours ; utilisez omnivox_file pour lire un fichier.','Enumera los metadatos de los documentos de Léa de un curso; usa omnivox_file para leer un archivo.'],
 omnivox_assignments:['Liste les devoirs Omnivox. Sans course_id, résume tous les cours.','Enumera las tareas de Omnivox. Sin course_id, resume todos los cursos.'],
 omnivox_grades:['Affiche les notes Omnivox. Sans course_id, résume tous les cours.','Muestra las notas de Omnivox. Sin course_id, resume todos los cursos.'],
 omnivox_calendar:['Affiche le calendrier courant ; omnivox_calendar_page et omnivox_calendar_month offrent la pagination et le choix du mois.','Muestra el calendario actual; omnivox_calendar_page y omnivox_calendar_month ofrecen paginación y selección de mes.'],
 omnivox_deadlines:['Rassemble les échéances Léa à venir, triées par date. Passez course_ids pour limiter le parcours.','Reúne las próximas fechas límite de Léa, ordenadas por fecha. Pasa course_ids para limitar el recorrido.'],
 omnivox_terms:['Découvre les identifiants de trimestre disponibles et le trimestre courant.','Descubre los identificadores de período disponibles y el período actual.'],
 omnivox_documents_summary:['Résume les nouveaux documents par cours, sans télécharger les fichiers.','Resume los documentos nuevos por curso, sin descargar archivos.'],
 omnivox_assignment_detail:['Détaille un devoir : consignes, échéance et fichiers de remise ou de correction.','Detalla una tarea: instrucciones, fecha límite y archivos de entrega o corrección.'],
 omnivox_announcements:['Compte les annonces par cours, sans afficher leur contenu.','Cuenta los anuncios por curso, sin mostrar su contenido.'],
 omnivox_course_announcements:['Affiche le texte des annonces d’un cours.','Muestra el texto de los anuncios de un curso.'],
 omnivox_absences:['Liste les absences par cours avec dates et heures.','Enumera las ausencias por curso con fechas y horas.'],
 omnivox_teachers:['Liste les enseignants des cours de l’étudiant.','Enumera los docentes de los cursos del estudiante.'],
 omnivox_teacher_detail:['Affiche les coordonnées et disponibilités publiées d’un enseignant.','Muestra los datos de contacto y la disponibilidad publicada de un docente.'],
 omnivox_websites:['Résume les sites Web d’enseignants ; peut contenir des compteurs, pas les liens.','Resume los sitios web de docentes; puede contener contadores, no enlaces.'],
 omnivox_schedule_week:['Affiche l’horaire d’une semaine précise à partir du lundi week_start.','Muestra el horario de una semana concreta desde el lunes week_start.'],
 omnivox_calendar_page:['Affiche une page du calendrier avec offset, type, trimestre et filtres de cours.','Muestra una página del calendario con offset, tipo, período y filtros de curso.'],
 omnivox_calendar_month:['Affiche le calendrier à partir du mois month_start.','Muestra el calendario a partir del mes month_start.'],
 omnivox_cancelled_classes:['Liste les avis de cours annulés.','Enumera los avisos de clases canceladas.'],
 omnivox_notifications:['Affiche les compteurs du tableau de bord, sans masquer les notifications.','Muestra los contadores del panel, sin ocultar notificaciones.'],
 omnivox_services:['Liste les modules disponibles et leur état bloqué ou en maintenance.','Enumera los módulos disponibles y su estado bloqueado o en mantenimiento.'],
 omnivox_college_news:['Affiche les dernières nouvelles de l’établissement, avec une limite bornée.','Muestra las últimas noticias de la institución, con un límite acotado.'],
 omnivox_communities:['Liste les communautés et les filtres de calendrier disponibles.','Enumera las comunidades y los filtros de calendario disponibles.'],
 omnivox_college_events:['Affiche les cartes d’événements de l’établissement rendues par le serveur.','Muestra las tarjetas de eventos de la institución renderizadas por el servidor.'],
 omnivox_college_documents:['Liste les documents, messages et contrats officiels, métadonnées seulement.','Enumera documentos, mensajes y contratos oficiales, solo metadatos.'],
 omnivox_forms:['Liste les formulaires en ligne sans les ouvrir ni les remplir.','Enumera los formularios en línea sin abrirlos ni responderlos.'],
 omnivox_mio_folders:['Liste les dossiers MIO avec les compteurs lus et non lus.','Enumera las carpetas MIO con contadores leídos y no leídos.'],
 omnivox_mio_messages:['Affiche une page de messages d’un dossier MIO.','Muestra una página de mensajes de una carpeta MIO.'],
 omnivox_mio_message:['Lit un message MIO complet, ses destinataires et ses pièces jointes, sans le marquer lu.','Lee un mensaje MIO completo, sus destinatarios y adjuntos, sin marcarlo como leído.'],
 omnivox_mio_search:['Recherche du texte dans un dossier MIO.','Busca texto en una carpeta MIO.'],
 omnivox_mio_contacts:['Liste les contacts MIO enregistrés ou récents, sans envoyer de message.','Enumera los contactos MIO guardados o recientes, sin enviar mensajes.'],
 omnivox_mio_people:['Recherche des destinataires dans l’annuaire MIO.','Busca destinatarios en el directorio MIO.'],
 omnivox_file:['Télécharge un document Léa ou MIO original (12 Mo). text ou images envoie texte et images de toutes les pages sélectionnées (4 par défaut, 8 maximum). Office : PDF local isolé, sinon blocs texte avec limites. Réutilisez documentId comme document_id pour le même compte ; suivez nextPageFrom.','Descarga un documento original de Léa o MIO (12 MB). text o images envía texto e imágenes de todas las páginas seleccionadas (4 por defecto, 8 máximo). Office: PDF local aislado o bloques de texto con limitaciones. Reutiliza documentId como document_id para la misma cuenta; sigue nextPageFrom.'],
 omnivox_mio_send:['Envoie, répond, répond à tous ou transfère un message à des destinataires explicites (écriture).','Envía, responde, responde a todos o reenvía un mensaje a destinatarios explícitos (escritura).'],
 omnivox_mio_move:['Classe un message dans une catégorie personnalisée ou retire sa catégorie (écriture).','Clasifica un mensaje en una categoría personalizada o quita su categoría (escritura).'],
 omnivox_mio_flags:['Définit les indicateurs favori et à relire d’un message (écriture).','Define los indicadores destacado y releer de un mensaje (escritura).'],
 omnivox_mio_delete:['Déplace un message vers la corbeille (écriture).','Mueve un mensaje a la papelera (escritura).'],
 omnivox_mio_delete_permanently:['Supprime définitivement un message, sans corbeille (écriture irréversible).','Elimina un mensaje de forma permanente, sin papelera (escritura irreversible).'],
 omnivox_mio_restore:['Restaure un message depuis la corbeille (écriture).','Restaura un mensaje desde la papelera (escritura).'],
 omnivox_mio_mark_read:['Marque un message comme lu ; ne permet pas de le remettre non lu (écriture).','Marca un mensaje como leído; no permite volver a no leído (escritura).'],
 omnivox_mio_folder_create:['Crée une catégorie MIO personnalisée (écriture).','Crea una categoría MIO personalizada (escritura).'],
 omnivox_mio_folder_rename:['Renomme une catégorie MIO personnalisée (écriture).','Renombra una categoría MIO personalizada (escritura).'],
 omnivox_mio_folder_delete:['Supprime une catégorie MIO en choisissant une catégorie de transfert ou aucune (écriture).','Elimina una categoría MIO eligiendo una categoría de transferencia o ninguna (escritura).'],
 omnivox_notification_dismiss:['Masque une notification du tableau de bord (écriture).','Oculta una notificación del panel (escritura).'],
 omnivox_assignment_mark_consulted:['Enregistre la consultation d’un devoir, sans déposer de fichier (écriture).','Registra la consulta de una tarea, sin subir archivos (escritura).'],
 brightspace_status:['Vérifie le compte Brightspace / myCourses connecté.','Comprueba la cuenta conectada de Brightspace / myCourses.'],
 brightspace_my_courses:['Liste une page des cours Brightspace de l’étudiant. Utilisez PagingInfo.Bookmark si HasMoreItems est vrai.','Enumera una página de cursos de Brightspace del estudiante. Usa PagingInfo.Bookmark si HasMoreItems es verdadero.'],
 brightspace_course_content:['Affiche la table des matières et les ressources d’un cours. Utilisez l’Id d’un sujet fichier (pas celui d’un module) comme topicid dans brightspace_file.','Muestra el índice y los recursos de un curso. Usa el Id de un tema de archivo (no el de un módulo) como topicid en brightspace_file.'],
 brightspace_file:['Télécharge l’original avec courseid et topicid. text ou images envoie le texte et chaque page en image (4 pages par défaut, 8 maximum). Office : conversion PDF locale isolée, sinon blocs texte avec limites ; tableurs en cellules. Réutilisez documentId comme document_id pour le même compte ; suivez nextPageFrom. Aucun lien externe téléchargé ; jetons et cookies restent privés.','Descarga el original con courseid y topicid. text o images envía texto y una imagen de cada página (4 por defecto, 8 máximo). Office: conversión local aislada a PDF o bloques de texto con limitaciones; hojas de cálculo en celdas. Reutiliza documentId como document_id para la misma cuenta; sigue nextPageFrom. No descarga enlaces externos; tokens y cookies siguen privados.'],
 brightspace_grades:['Affiche les notes visibles de l’étudiant dans un cours Brightspace.','Muestra las notas visibles del estudiante en un curso de Brightspace.'],
 brightspace_assignments:['Liste les dossiers de remise des devoirs d’un cours, avec leurs dates d’échéance. Pour une vue globale, utilisez brightspace_deadlines.','Enumera las carpetas de entrega de tareas de un curso con sus fechas límite. Para una vista general, usa brightspace_deadlines.'],
 brightspace_assignment_status:['Affiche un dossier de remise, les travaux déposés et les commentaires reçus.','Muestra una carpeta de entrega, los trabajos enviados y los comentarios recibidos.'],
 brightspace_assignment_submit:['Dépose un fichier dans un dossier de devoir. Fournissez courseid et folderid, puis exactement une source : path (fichier local sur l’hôte student-mcp), content_base64 avec filename, ou text. comment est facultatif. Confirmez avec l’utilisateur avant tout dépôt : cette action est irréversible pour le cours. Maximum 12 Mo.','Entrega un archivo en una carpeta de tareas. Indica courseid y folderid, y exactamente una fuente: path (archivo local en el equipo student-mcp), content_base64 con filename, o text. comment es opcional. Confirma con el usuario antes de entregar: esta acción es irreversible para el curso. Máximo 12 MB.'],
 brightspace_quizzes:['Liste les quiz d’un cours, avec leurs dates d’échéance, d’ouverture et de clôture.','Enumera los cuestionarios de un curso con sus fechas de vencimiento, inicio y cierre.'],
 brightspace_quiz_status:['Affiche la définition d’un quiz et les tentatives de l’étudiant si disponibles.','Muestra la definición de un cuestionario y los intentos del estudiante, si están disponibles.'],
 brightspace_calendar:['Affiche les événements du calendrier à venir. courseid filtre un cours ; days vaut 60 par défaut.','Muestra los próximos eventos del calendario. courseid filtra un curso; days vale 60 por defecto.'],
 brightspace_deadlines:['Affiche les prochaines échéances des devoirs, quiz et événements, triées par date.','Muestra las próximas fechas límite de tareas, cuestionarios y eventos, ordenadas por fecha.'],
 brightspace_news:['Liste les annonces d’un cours Brightspace.','Enumera los anuncios de un curso de Brightspace.'],
 brightspace_api_versions:['Affiche les versions de l’API prises en charge par l’établissement, sans garantir les permissions du compte.','Muestra las versiones de la API compatibles con la institución, sin garantizar los permisos de la cuenta.'],
 brightspace_discussion_forums:['Liste les forums de discussion visibles d’un cours.','Enumera los foros de debate visibles de un curso.'],
 brightspace_discussion_topics:['Liste les sujets visibles d’un forum.','Enumera los temas visibles de un foro.'],
 brightspace_discussion_posts:['Lit une page de messages d’un sujet. pageNumber commence à 1 ; pageSize vaut 50 par défaut (100 maximum). Parcourez les pages suivantes jusqu’à ce qu’elles soient vides.','Lee una página de publicaciones de un tema. pageNumber empieza en 1; pageSize vale 50 por defecto (máximo 100). Recorre las siguientes páginas hasta que estén vacías.'],
 brightspace_discussion_post:['Lit un message de discussion.','Lee una publicación de un debate.'],
 brightspace_discussion_post_create:['Publie un nouveau fil ou une réponse (parentpostid). Texte brut uniquement, sans pièces jointes ni messages anonymes ; les sujets de groupe ne sont pas pris en charge. Écriture autorisée requise : demandez confirmation avant de publier.','Publica un nuevo hilo o una respuesta (parentpostid). Solo texto plano, sin adjuntos ni publicaciones anónimas; no admite temas de grupo. Requiere permiso de escritura: pide confirmación antes de publicar.'],
 brightspace_discussion_post_delete:['Supprime un message si le compte y est autorisé. Action destructive : ne réessayez pas automatiquement ; demandez confirmation.','Elimina una publicación si la cuenta tiene permiso. Acción destructiva: no vuelvas a intentarlo automáticamente; pide confirmación.'],
 brightspace_discussion_read_status:['Marque un message de discussion comme lu ou non lu (écriture).','Marca una publicación de debate como leída o no leída (escritura).'],
 brightspace_content_due:['Affiche une page de contenus prévus encore à faire, hors exemptions. Utilisez Next/bookmark pour les pages suivantes.','Muestra una página de contenidos programados aún pendientes, sin exenciones. Usa Next/bookmark para las páginas siguientes.'],
 brightspace_content_recent:['Liste les sujets de cours récemment consultés.','Enumera los temas del curso visitados recientemente.'],
 brightspace_content_bookmarks:['Lit les sujets enregistrés en favoris ; ne crée pas de favoris.','Lee los temas guardados como marcadores; no crea marcadores.'],
 brightspace_content_completion:['Affiche le total des contenus terminés et requis de l’étudiant.','Muestra el total de contenidos completados y obligatorios del estudiante.'],
 brightspace_course_overview:['Affiche la présentation du cours et les métadonnées de ses pièces jointes.','Muestra el resumen del curso y los metadatos de sus archivos adjuntos.'],
 brightspace_content_topic:['Affiche les détails d’un sujet de contenu.','Muestra los detalles de un tema de contenido.'],
 brightspace_search:['Recherche les titres et URL dans la table des matières accessible d’un cours. Ne recherche ni le texte intégral des PDF ni tous les cours.','Busca títulos y URL en el índice accesible de un curso. No busca en el texto completo de los PDF ni en todos los cursos.'],
 brightspace_grade_final:['Affiche la note finale calculée de l’étudiant, si l’établissement l’a publiée.','Muestra la nota final calculada del estudiante, si la institución la ha publicado.'],
 brightspace_grade_value:['Affiche la note d’un élément et les commentaires visibles.','Muestra la nota de un elemento y los comentarios visibles.'],
 brightspace_grade_categories:['Liste les catégories de notes visibles, selon les permissions du compte.','Enumera las categorías de notas visibles, según los permisos de la cuenta.'],
 brightspace_course_updates:['Affiche les compteurs de nouveautés d’un cours, pas les courriels privés.','Muestra los contadores de novedades de un curso, no los correos privados.'],
 brightspace_alerts:['Affiche une page des alertes de l’utilisateur, avec pagination. Ne comprend pas les courriels privés.','Muestra una página de alertas del usuario, con paginación. No incluye correos privados.'],
 brightspace_my_groups:['Affiche une page des groupes de l’étudiant. Nécessite LP 1.64.','Muestra una página de los grupos del estudiante. Requiere LP 1.64.'],
 brightspace_groups_available:['Affiche une page des groupes ouverts à l’inscription. Nécessite LP 1.64 ; n’inscrit pas l’étudiant.','Muestra una página de grupos disponibles para inscribirse. Requiere LP 1.64; no inscribe al estudiante.'],
 brightspace_group_categories:['Liste les catégories de groupes visibles d’un cours.','Enumera las categorías de grupos visibles de un curso.'],
 brightspace_category_groups:['Liste les groupes visibles d’une catégorie et leurs inscriptions, si autorisées.','Enumera los grupos visibles de una categoría y sus inscripciones, si están permitidas.'],
 brightspace_group_join:['Inscrit uniquement l’étudiant connecté à un groupe, si ses permissions le permettent. L’accès peut être refusé même si le groupe est visible.','Inscribe solo al estudiante conectado en un grupo, si sus permisos lo permiten. Puede denegarse el acceso aunque el grupo sea visible.'],
 brightspace_group_leave:['Retire uniquement l’inscription de l’étudiant connecté à un groupe, si autorisé (écriture).','Elimina solo la inscripción del estudiante conectado en un grupo, si está permitido (escritura).'],
 brightspace_my_sections:['Liste les sections du cours auxquelles l’étudiant appartient.','Enumera las secciones del curso a las que pertenece el estudiante.'],
 brightspace_classlist:['Affiche les participants visibles d’un cours, sous réserve des règles de confidentialité de l’établissement.','Muestra los participantes visibles de un curso, según las reglas de privacidad de la institución.'],
 brightspace_checklists:['Liste les listes de contrôle d’un cours, sans l’état de validation de l’étudiant.','Enumera las listas de verificación de un curso, sin el estado de progreso del estudiante.'],
 brightspace_checklist_items:['Liste les éléments d’une liste de contrôle ; ne les marque pas comme terminés.','Enumera los elementos de una lista de verificación; no los marca como completados.'],
 brightspace_surveys:['Liste les sondages visibles, sans y répondre ni les soumettre.','Enumera las encuestas visibles, sin responderlas ni enviarlas.'],
 brightspace_news_item:['Lit une annonce complète et les métadonnées de ses pièces jointes.','Lee un anuncio completo y los metadatos de sus archivos adjuntos.'],
 brightspace_news_dismiss:['Masque une annonce pour l’étudiant connecté (écriture).','Oculta un anuncio para el estudiante conectado (escritura).'],
 brightspace_news_restore:['Réaffiche une annonce masquée ; ne restaure pas une annonce supprimée du cours.','Vuelve a mostrar un anuncio oculto; no restaura un anuncio eliminado del curso.'],
 brightspace_course_pin:['Épingle un cours parmi les inscriptions de l’étudiant (écriture).','Fija un curso entre las inscripciones del estudiante (escritura).'],
 brightspace_course_unpin:['Désépingle un cours parmi les inscriptions de l’étudiant (écriture).','Desfija un curso de las inscripciones del estudiante (escritura).'],
 brightspace_my_profile:['Affiche le profil personnel de l’étudiant, sans ses identifiants de connexion.','Muestra el perfil personal del estudiante, sin sus credenciales de acceso.'],
 brightspace_my_locale:['Affiche les préférences de langue et de région du compte.','Muestra las preferencias de idioma y región de la cuenta.'],
 brightspace_notification_channels:['Liste les canaux disponibles pour recevoir des notifications.','Enumera los canales disponibles para recibir notificaciones.'],
 brightspace_notification_subscriptions:['Liste les abonnements de l’étudiant pour un canal de notification.','Enumera las suscripciones del estudiante para un canal de notificaciones.'],
 brightspace_notification_subscribe:['Abonne le compte connecté à un type de message sur un canal de notification (écriture).','Suscribe la cuenta conectada a un tipo de mensaje en un canal de notificaciones (escritura).'],
 brightspace_notification_unsubscribe:['Désabonne le compte connecté d’un type de message sur un canal de notification (écriture).','Cancela la suscripción de la cuenta conectada a un tipo de mensaje en un canal de notificaciones (escritura).']
};
function t(text){
 if(language==='en')return text;
 const index=language==='fr'?0:1;
 if(translations[text])return translations[text][index];
 const connect=text.match(/^Connect to (Moodle|Omnivox|Brightspace|Canvas)$/);
 if(connect)return (index===0?'Se connecter à ':'Conectar con ')+connect[1];
 const service=text.match(/^(Moodle|Omnivox|Brightspace|Canvas) (manage connection|connect)$/);
 if(service)return service[1]+' — '+(service[2]==='connect'?['Se connecter','Conectar'][index]:['Gérer la connexion','Gestionar conexión'][index]);
 const help=text.match(/^(Hide|Show)( help for)? (.+)$/);
 if(help)return (help[1]==='Hide'?['Masquer','Ocultar'][index]:['Afficher','Mostrar'][index])+(help[2]?[' l’aide pour ',' ayuda para '][index]:' ')+t(help[3]);
 const emailPrompt=text.match(/^Enter the 6-digit security code sent by email(?: to (.+))?\.$/);
 if(emailPrompt)return (index===0?'Saisissez le code de sécurité à 6 chiffres envoyé par courriel':'Introduce el código de seguridad de 6 dígitos enviado por correo')+(emailPrompt[1]?(index===0?' à ':' a ')+emailPrompt[1]:'')+'.';
 return text;
}
function localize(screen){
 const walker=document.createTreeWalker(screen,NodeFilter.SHOW_TEXT);
 while(walker.nextNode()){
  const node=walker.currentNode;
  if(node.parentElement.closest('.account-row-title,.profile-label,.link-detail,.notice,.hint,[data-server-text]'))continue;
  const text=node.textContent.trim();
  if(text)node.textContent=node.textContent.replace(text,t(text));
 }
 for(const el of screen.querySelectorAll('[aria-label],[placeholder]'))for(const attr of ['aria-label','placeholder']){
  if(el.hasAttribute(attr)&&!el.hasAttribute('data-account'))el.setAttribute(attr,t(el.getAttribute(attr)));
 }
}
function updateLanguageControl(){
 document.documentElement.lang=language;
 document.title=t('Student MCP');
 $('language-button').setAttribute('aria-label',t('Change language'));
 $('language-menu').setAttribute('aria-label',t('Language'));
 $('lightbox').setAttribute('aria-label',t('Screenshot'));
 $('lightbox').querySelector('[data-lightbox-close]').setAttribute('aria-label',t('Close'));
 for(const button of document.querySelectorAll('[data-language]'))button.setAttribute('aria-pressed',String(button.dataset.language===language));
}
function closeLanguages(restoreFocus=false){$('language-menu').hidden=true;$('language-button').setAttribute('aria-expanded','false');if(restoreFocus)$('language-button').focus()}
// The language card spans the startup switch and the language button, so their outer edges line up.
function sizeLanguageMenu(){const startup=$('app-startup'),button=$('language-button'),menu=$('language-menu');const left=startup?.getBoundingClientRect().left,right=button?.getBoundingClientRect().right;if(left!=null&&right!=null&&right>left)menu.style.width=Math.round(right-left)+'px'}
window.addEventListener('resize',sizeLanguageMenu);document.fonts?.ready.then(sizeLanguageMenu);
document.addEventListener('click',e=>{
 const choice=e.target.closest('[data-language]');
 if(choice){language=choice.dataset.language;try{localStorage.setItem('student-mcp.language',language)}catch{}updateLanguageControl();render();closeLanguages(true);return}
 if(e.target.closest('#language-button')){const open=$('language-menu').hidden;$('language-menu').hidden=!open;$('language-button').setAttribute('aria-expanded',String(open));if(open)$('language-menu').querySelector('[aria-pressed="true"]').focus();return}
 if(!e.target.closest('.language-control'))closeLanguages();
});
$('lightbox').addEventListener('close',()=>{
 document.body.classList.remove('lightbox-open');
 const trigger=[...document.querySelectorAll('[data-lightbox]')].find(button=>button.dataset.lightbox===$('lightbox').dataset.trigger);
 $('lightbox-image').removeAttribute('src');trigger?.focus({preventScroll:true});
});
document.addEventListener('keydown',e=>{if(e.key==='Escape'&&!$('language-menu').hidden){e.preventDefault();closeLanguages(true)}});
document.addEventListener('focusin',e=>{if(!e.target.closest('.language-control'))closeLanguages()});
updateLanguageControl();
const state={screen:'home',service:'',assistant:'',assistantStatus:null,assistantBusy:false,assistantConfirm:false,assistantRemoveConfirm:false,accountId:'',editing:false,renaming:false,accounts:[],draft:{},visible:{},help:{},tunnel:'Idle',link:'',localLink:'',public:false,local:true,autostart:false,notice:null,busy:false,verification:null,otherSchool:false,copied:false,health:null,manualLogin:false,browserLogin:null,confirmDisconnect:false,schoolIndex:-1};
const toolCatalogs={};
async function loadTools(service){
 if(toolCatalogs[service]?.loading||toolCatalogs[service]?.tools)return;
 toolCatalogs[service]={loading:true};render();
 try{const result=await api('/tools/'+encodeURIComponent(service));toolCatalogs[service]={tools:result.tools}}
 catch{toolCatalogs[service]={error:true}}
 if(state.screen==='tools'&&state.service===service)render();
}
function toolsScreen(){
 const catalog=toolCatalogs[state.service];
 const content=catalog?.error?`<div class="notice error" role="alert">${t('Could not load tools.')}</div><button class="full-button secondary" data-tools-retry>Retry</button>`:!catalog?.tools?'<p class="busy" role="status">Loading tools…</p>':catalog.tools.length?`<div class="tools-list">${catalog.tools.map(tool=>{
  const translated=language!=='en'&&toolDescriptions[tool.name]?.[language==='fr'?0:1];
  return `<article class="tool-card"><h2>${esc(tool.name)}</h2><p lang="${translated?language:'en'}" data-server-text>${esc(translated||tool.description)}</p></article>`
 }).join('')}</div>`:'<p class="subtitle">No tools available.</p>';
 return `<section><header class="tools-heading">${pageNav('service','Back to '+names[state.service])}<h1>MCP tools</h1><p class="subtitle">${names[state.service]}${catalog?.tools?' · '+catalog.tools.length:''}</p></header>${content}</section>`;
}
const assistantInfo={
 chatgpt:{name:'ChatGPT',accent:'#0f9d78',settings:'https://chatgpt.com/plugins',guide:'https://developers.openai.com/plugins/deploy/connect-chatgpt'},
 claude:{name:'Claude',accent:'#c96442',local:true,settings:'https://claude.ai/customize/connectors',guide:'https://support.claude.com/en/articles/11175166-get-started-with-custom-connectors-using-remote-mcp'},
 grok:{name:'Grok',accent:'#1d1d1f',settings:'https://grok.com/connectors',guide:'https://docs.x.ai/grok/connectors'},
 gemini:{name:'Gemini',accent:'#3b82f6',local:true,settings:'https://gemini.google.com/apps',guide:'https://support.google.com/gemini/answer/17209137?hl=en&co=GENIE.Platform%3DDesktop'}
};
const assistantJourney={step:0};
const localAssistants={mode:'web',items:null,scanning:false,error:null,selected:null};
function assistantModes(){return `<nav class="assistant-modes" aria-label="${t('Choose a local app or the web.')}">${['web','local'].map(mode=>`<button data-assistant-mode="${mode}" aria-pressed="${localAssistants.mode===mode}" ${state.assistantBusy?'disabled':''}>${t(mode==='web'?'Web':'On this computer')}</button>`).join('')}</nav>`}
function assistantSwitcher(){return `<nav class="assistant-switcher" aria-label="${t('Your assistant')}">${Object.entries(assistantInfo).map(([id,info])=>`<button data-assistant="${id}" aria-pressed="${state.assistant===id}" aria-label="${t('Set up '+info.name)}"><img src="/assets/${id}.png" alt=""><span>${info.name}</span></button>`).join('')}</nav>`}
function publicGuideLink(label='Your public link'){const ready=state.public&&state.tunnel.startsWith('Ready(')&&/^https:\/\//.test(state.link);return `<div class="setup-link"><div><span class="setup-link-label">${t(label)}</span>${ready?`<code>${esc(state.link)}</code>`:`<span>${t('Enable your public link')}</span>`}</div>${ready?`<button class="copy-pill" data-copy="assistant">${icon('copy')}<span>${t('Copy link')}</span></button>`:`<button class="copy-pill" data-screen="links">${t('Open link settings')}</button>`}</div>${ready?'':`<p class="setup-note">${t('Your public link is not ready. Open link settings and turn on From the web.')}</p>`}`}
function guideCapture(file,caption){const path='/assets/'+file+'.webp';return `<figure class="guide-capture"><button class="guide-shot-link" type="button" data-lightbox="${path}" aria-label="${t('Enlarge screenshot')}"><img class="guide-shot" src="${path}" alt="${esc(t(caption))}" width="1440" height="1080" loading="lazy" decoding="async"></button></figure>`}
 function guideFields(fields){return `<dl class="journey-fields">${fields.map(([label,value])=>`<div class="journey-field"><dt>${t(label)}</dt><dd>${esc(t(value))}</dd></div>`).join('')}</dl>`}
 function guidePath(items){return `<ol class="guide-path" aria-label="${t('Click path')}">${items.map(text=>`<li><span data-server-text>${esc(t(text))}</span></li>`).join('')}</ol>`}
function guideOpen(id){return `<a class="full-button primary guide-link" href="${assistantInfo[id].settings}" target="_blank" rel="noopener noreferrer">${t('Open '+assistantInfo[id].name+' settings')} <span aria-hidden="true">↗</span></a>`}
function assistantLocal(id){
 const status=state.assistantStatus,name=localAssistants.items?.find(app=>app.assistant===id)?.name||assistantInfo[id]?.name||id;
 if(!status)return `<div class="assistant-local" id="assistant-local"><p class="busy" role="status">${t('Checking local setup…')}</p><button class="local-close" data-assistant-cancel>${t('Cancel')}</button></div>`;
 if(!status.detected)return `<div class="assistant-local local-unavailable" id="assistant-local"><div class="assistant-local-copy" role="status"><i class="status-dot offline" aria-hidden="true"></i><strong>${esc(name)} · ${t('Not detected')}</strong></div><button class="local-close" data-assistant-cancel>${t('Close')}</button></div>`;
 if(status.configured){
  const reload=status.reload==='automatic'?t('Settings apply automatically in OpenCode.'):status.reload==='mcp'?t('Reload MCP in the app to use the saved settings.'):t('Restart the assistant to finish setup.');
  if(state.assistantRemoveConfirm)return `<div class="assistant-local" id="assistant-local"><div class="assistant-confirm"><p>${t('Remove student-mcp from this app? Your other MCP servers stay unchanged and a backup is created.')}</p><div class="assistant-confirm-actions"><button class="full-button secondary" data-assistant-remove-cancel ${state.assistantBusy?'disabled':''}>${t('Cancel')}</button><button class="full-button danger" data-assistant-remove-confirm="${id}" ${state.assistantBusy?'disabled':''}>${state.assistantBusy?t('Removing…'):t('Remove setup')}</button></div></div></div>`;
  return `<div class="assistant-local" id="assistant-local"><div class="assistant-local-copy"><i class="status-dot" aria-hidden="true"></i><div><strong>${esc(name)} · ${t('Configured')}</strong><p>${reload}</p></div></div><div class="assistant-actions"><button class="text-button" data-assistant-remove="${id}" ${state.assistantBusy?'disabled':''}>${t('Remove setup')}</button><button class="local-close" data-assistant-cancel>${t('Close')}</button></div></div>`;
 }
 return `<div class="assistant-local" id="assistant-local"><div class="assistant-local-copy"><div><strong>${esc(name)}</strong></div></div><div class="assistant-confirm"><p>${t('This updates the assistant’s MCP settings. Your other servers stay unchanged and a backup is created.')}</p><div class="assistant-confirm-actions"><button class="full-button secondary" data-assistant-cancel ${state.assistantBusy?'disabled':''}>${t('Cancel')}</button><button class="full-button primary" data-assistant-confirm="${id}" ${state.assistantBusy?'disabled':''}>${state.assistantBusy?t('Connecting…'):t('Confirm setup')}</button></div></div></div>`;
}

const localLogos={'claude':'/assets/claude.png','claude-code':'/assets/local-claude-code.png',codex:'/assets/chatgpt.png',gemini:'/assets/gemini.png','grok-build':'/assets/grok.png',opencode:'/assets/local-opencode.png',cursor:'/assets/local-cursor.png',vscode:'/assets/local-vscode.png',windsurf:'/assets/local-windsurf.png',qwen:'/assets/local-qwen.png',pi:'/assets/local-pi.png',omp:'/assets/local-omp.png',kiro:'/assets/local-kiro.png',amp:'/assets/local-amp.png',crush:'/assets/local-crush.png',devin:'/assets/local-devin.png'};
function localAppCard(app){
 const selected=localAssistants.selected===app.assistant,mark=app.name.replace(/[^A-Za-z0-9]/g,'').slice(0,2),logo=localLogos[app.assistant];
 const visual=logo?`<img class="local-app-logo" src="${logo}" alt="" width="128" height="128" loading="lazy" decoding="async">`:`<span class="local-app-mark" aria-hidden="true">${esc(mark)}</span>`;
 const statusText=t(!app.detected?'Not detected':app.configured?'Configured':'Detected');
 return `<li><button class="local-app-card${app.configured?' is-ready':''}" data-local-assistant="${esc(app.assistant)}" aria-pressed="${selected}" aria-controls="assistant-local" ${!app.detected||state.assistantBusy||localAssistants.scanning?'disabled':''}>${visual}<span class="local-app-copy"><span class="local-app-name">${esc(app.name)}</span></span><span class="local-app-state">${statusText}</span><span class="local-app-arrow" aria-hidden="true">›</span></button></li>`;
}
function localAppsScreen(){
 const items=localAssistants.items||[],found=items.filter(app=>app.detected),missing=items.filter(app=>!app.detected);
 return `<section class="local-apps" aria-labelledby="local-apps-title"><header class="local-apps-head"><h2 id="local-apps-title" class="section-label">${t('Choose an assistant app')}</h2><button class="local-scan" data-assistant-scan ${state.assistantBusy||localAssistants.scanning?'disabled':''}>${icon('refresh')}<span>${t(localAssistants.scanning?'Scanning…':'Scan again')}</span></button></header><div role="status" aria-live="polite">${localAssistants.scanning?`<p class="busy">${t('Scanning…')}</p>`:''}${localAssistants.error?`<p class="local-scan-error" role="alert">${esc(localAssistants.error)}</p>`:''}${!localAssistants.scanning&&localAssistants.items&&!found.length?`<p class="local-note">${t('No supported local app was found.')}</p>`:''}</div><ul class="local-app-list">${found.map(localAppCard).join('')}</ul>${localAssistants.selected?assistantLocal(localAssistants.selected):''}${missing.length?`<details class="other-apps"><summary>${t('Other apps')} (${missing.length})</summary><ul class="local-app-list is-muted">${missing.map(localAppCard).join('')}</ul></details>`:''}</section>`;
}

async function scanAssistants(){if(localAssistants.scanning||state.assistantBusy)return;localAssistants.scanning=true;localAssistants.error=null;localAssistants.selected=null;state.assistantConfirm=false;render();try{const result=await api('/assistants');localAssistants.items=result.assistants}catch(error){localAssistants.error=error.message}finally{localAssistants.scanning=false;if(state.screen==='assistant')render()}}
function assistantGuide(id){
 const step=assistantJourney.step;
 if(step===0){
  const observed={
   grok:guideCapture('guide-grok-settings','Grok · Plugins → New Connector'),
   claude:guideCapture('guide-claude-menu','Claude · Add → Add custom connector'),
   chatgpt:guideCapture('guide-chatgpt-menu','ChatGPT · Add → Add custom MCP server'),
   gemini:guideCapture('guide-gemini-settings','Gemini · Connected Apps → + Custom')
  };
  return {title:'Open the connector settings',content:guideOpen(id)+observed[id]};
 }
 if(step===1){
  const forms={
    grok:guideFields([['Name','student-mcp']])+publicGuideLink('Server URL')+guideCapture('guide-grok-form','Grok · Custom Connector'),
   claude:publicGuideLink('Remote MCP server URL')+guideFields([['Name','student-mcp'],['Authentication','No sign in']])+guidePath(['Continue','No sign in','Add'])+guideCapture('guide-claude-form','Claude · Add custom connector'),
   chatgpt:publicGuideLink('Connection → Server URL')+guideFields([['Name','student-mcp'],['Authentication','No authentication']])+guideCapture('guide-chatgpt-form','ChatGPT · Add custom MCP server'),
   gemini:publicGuideLink('MCP Server URL')+guideCapture('guide-gemini-form','Gemini · MCP Server URL → Next')
  };
  return {title:'Add student-mcp',content:forms[id]};
 }
 const paths={grok:[],claude:['+','Connectors','student-mcp'],chatgpt:['@','student-mcp'],gemini:['@',t('Your custom app')]};
 const chats={grok:'guide-grok-chat',claude:'guide-claude-chat',chatgpt:'guide-chatgpt-chat',gemini:'guide-gemini-chat'};
 return {title:'Ask your first question',content:(paths[id].length?guidePath(paths[id]):'')+`<blockquote class="setup-prompt"><p>${t('Use student-mcp: what are my deadlines this week?')}</p></blockquote>`+guideCapture(chats[id],'Write in the message field')};
}
function assistantScreen(){
  const id=state.assistant,info=assistantInfo[id],step=assistantJourney.step;
  const heading=`<header class="assistant-heading"><div class="page-nav"><button class="back" data-screen="home" aria-label="${t('Back to services')}">${icon('back')}</button></div><h1>${t('Set up an assistant')}</h1></header>${assistantModes()}`;
  if(localAssistants.mode==='local')return `<section style="--accent:var(--blue)">${heading}${localAppsScreen()}${notice()}</section>`;
 const labels=['Open settings','Add the server','Ask a question'],short=['Settings','Connect','Try it'];
 const guide=assistantGuide(id);
  return `<section style="--accent:${info.accent}">${heading}<div class="assistant-toolbar">${assistantSwitcher()}</div><div class="assistant-workspace assistant-journey"><aside class="journey-sidebar"><ol class="journey-steps" aria-label="${t('Setup steps')}">${labels.map((label,index)=>`<li><button class="journey-node" data-guide-step="${index}"${index===step?' aria-current="step"':''} aria-controls="setup-stage" aria-label="${index+1}. ${t(label)}"><span class="journey-number" aria-hidden="true">${index+1}</span><span class="journey-long">${t(label)}</span><span class="journey-short" aria-hidden="true">${t(short[index])}</span></button></li>`).join('')}</ol></aside><article class="setup-stage" id="setup-stage" aria-labelledby="setup-title"><h2 id="setup-title" tabindex="-1">${t(guide.title)}</h2><div class="setup-content">${guide.content}</div><footer class="setup-footer"><div class="setup-footer-actions">${step?`<button class="setup-previous" data-guide-previous>${t('Previous')}</button>`:''}<button class="setup-next" ${step===2?'data-screen="home"':'data-guide-next'}>${t(step===2?'Finish guide':'Continue')} <span aria-hidden="true">→</span></button></div></footer></article></div>${notice()}</section>`;
}
async function openAssistant(id){if(!assistantInfo[id]||state.assistantBusy)return;localAssistants.mode='web';state.assistant=id;assistantJourney.step=0;state.assistantStatus=null;state.assistantConfirm=false;state.assistantRemoveConfirm=false;state.notice=null;state.screen='assistant';render()}
async function configureAssistant(id){if(state.assistantBusy||localAssistants.selected!==id||!state.assistantConfirm)return;state.assistantBusy=true;state.notice=null;render();try{const status=await api('/action',{action:'assistant_setup',assistant:id});state.assistantStatus=status;localAssistants.items=localAssistants.items.map(app=>app.assistant===id?{...app,...status}:app)}catch(error){state.notice={text:error.message,error:true}}finally{state.assistantBusy=false;if(state.screen==='assistant'){render();if(localAssistants.mode==='local'&&state.assistantStatus?.configured)document.querySelector('.assistant-local .local-close')?.focus({preventScroll:true})}}}
async function removeAssistant(id){
 if(state.assistantBusy||localAssistants.selected!==id||!state.assistantRemoveConfirm)return;
 state.assistantBusy=true;state.notice=null;render();
 try{
  const status=await api('/action',{action:'assistant_remove',assistant:id});
  state.assistantStatus=status;
  localAssistants.items=(localAssistants.items||[]).map(app=>app.assistant===id?{...app,...status}:app);
  state.assistantRemoveConfirm=false;
 }catch(error){state.notice={text:error.message,error:true}}
 finally{state.assistantBusy=false;if(state.screen==='assistant'){render();document.querySelector(`[data-local-assistant="${id}"]`)?.focus({preventScroll:true})}}
}
async function api(path,data){
 const response=await fetch(path,{method:data?'POST':'GET',headers:{Authorization:'Bearer '+key,'Content-Type':'application/json'},body:data?JSON.stringify(data):undefined});
 const text=await response.text();let value={};
 if(text){try{value=JSON.parse(text)}catch{throw Error(t('Request failed'))}}
 if(!response.ok)throw Error(value.error||t('Request failed'));
 return value;
}
const names={moodle:'Moodle',omnivox:'Omnivox',brightspace:'Brightspace',canvas:'Canvas'};
// 280 établissements vérifiés par HTTP (200 + titre du portail) le 2026-11-09.
// Un portail principal par établissement ; ajouter une ligne suffit.
const schools=[
 {n:'Acadia University',s:'moodle',u:'https://moodle.acadiau.ca',a:'acadia university acadiau'},
 {n:'Algonquin College',s:'brightspace',u:'https://brightspace.algonquincollege.com',a:'algonquin college ottawa algonquincollege'},
 {n:'Arizona State University',s:'canvas',u:'https://canvas.asu.edu',a:'arizona state university asu'},
 {n:'Athabasca University',s:'brightspace',u:'https://brightspace.athabascau.ca',a:'athabasca university athabascau'},
 {n:'Baylor University',s:'canvas',u:'https://canvas.baylor.edu',a:'baylor university'},
 {n:"Bishop's University",s:'moodle',u:'https://moodle.ubishops.ca',a:'bishops bishop university ubishops'},
 {n:'Boise State University',s:'canvas',u:'https://canvas.boisestate.edu',a:'boise state university boisestate'},
 {n:'Bordeaux INP',s:'moodle',u:'https://moodle.bordeaux-inp.fr',a:'bordeaux inp'},
 {n:'Bow Valley College',s:'brightspace',u:'https://d2l.bowvalleycollege.ca',a:'bow valley college calgary bowvalleycollege'},
 {n:'Brandeis University',s:'moodle',u:'https://moodle.brandeis.edu',a:'brandeis university'},
 {n:'Brandon University',s:'moodle',u:'https://moodle.brandonu.ca',a:'brandon university brandonu'},
 {n:'Brigham Young University',s:'canvas',u:'https://canvas.byu.edu',a:'brigham young university byu'},
 {n:'Brock University',s:'brightspace',u:'https://lms.brocku.ca',a:'brock university brocku'},
 {n:'Brown University',s:'canvas',u:'https://canvas.brown.edu',a:'brown university'},
 {n:'California Institute of Technology',s:'canvas',u:'https://canvas.caltech.edu',a:'california institute technology caltech'},
 {n:'California Polytechnic State University',s:'canvas',u:'https://canvas.calpoly.edu',a:'california polytechnic state university calpoly'},
 {n:'Cambrian College',s:'moodle',u:'https://moodle.cambriancollege.ca',a:'cambrian college sudbury cambriancollege'},
 {n:'Capilano University',s:'moodle',u:'https://moodle.capilanou.ca',a:'capilano university capilanou'},
 {n:'Carleton University',s:'brightspace',u:'https://brightspace.carleton.ca',a:'carleton university'},
 {n:'Carnegie Mellon University',s:'canvas',u:'https://canvas.cmu.edu',a:'carnegie mellon university cmu'},
 {n:'Case Western Reserve University',s:'canvas',u:'https://canvas.case.edu',a:'case western reserve university'},
 {n:'Cégep André-Laurendeau',s:'omnivox',u:'https://claurendeau.omnivox.ca',a:'claurendeau cegep andre laurendeau'},
 {n:'Cégep Beauce-Appalaches',s:'omnivox',u:'https://cegepba.omnivox.ca',a:'cegepba cegep beauce appalaches'},
 {n:'Cégep de Baie-Comeau',s:'omnivox',u:'https://cegep-baie-comeau.omnivox.ca',a:'cegep-baie-comeau cegep baie comeau baie-comeau'},
 {n:'Cégep de Bois-de-Boulogne',s:'omnivox',u:'https://bdeb.omnivox.ca',a:'bdeb bois-de-boulogne cegep bois boulogne'},
 {n:'Cégep de Chicoutimi',s:'omnivox',u:'https://cchic.omnivox.ca',a:'cegep chicoutimi cchic'},
 {n:'Cégep de Drummondville',s:'omnivox',u:'https://cegepdrummond.omnivox.ca',a:'cegepdrummond cegep drummondville'},
 {n:'Cégep de Granby Haute-Yamaska',s:'omnivox',u:'https://cegepgranby.omnivox.ca',a:'cegepgranby cegep granby haute yamaska'},
 {n:'Cégep de Jonquière',s:'moodle',u:'https://moodle.cegepjonquiere.ca',a:'cegepjonquiere cegep jonquiere'},
 {n:"Cégep de l'Abitibi-Témiscamingue",s:'omnivox',u:'https://cegepat.omnivox.ca',a:'cegepat cegep abitibi temiscamingue rouyn'},
 {n:"Cégep de l'Outaouais",s:'omnivox',u:'https://cegepoutaouais.omnivox.ca',a:'cegepoutaouais cegep outaouais gatineau hull'},
 {n:'Cégep de la Gaspésie et des Îles',s:'omnivox',u:'https://cegepgim.omnivox.ca',a:'cegepgim cegep gaspesie iles'},
 {n:'Cégep de Lévis',s:'omnivox',u:'https://cegeplevis.omnivox.ca',a:'cegeplevis cegep levis'},
 {n:'Cégep de Matane',s:'omnivox',u:'https://cegep-matane.omnivox.ca',a:'cegep-matane cegep matane'},
 {n:'Cégep de Rimouski',s:'omnivox',u:'https://cegep-rimouski.omnivox.ca',a:'cegep-rimouski cegep rimouski'},
 {n:'Cégep de Rivière-du-Loup',s:'omnivox',u:'https://cegeprdl.omnivox.ca',a:'cegeprdl cegep riviere loup riviere-du-loup'},
 {n:'Cégep de Rosemont',s:'omnivox',u:'https://crosemont.omnivox.ca',a:'cegep rosemont crosemont'},
 {n:'Cégep de Saint-Félicien',s:'moodle',u:'https://moodle.cegepstfe.ca',a:'stfe cegep saint felicien saint-felicien cegepstfe'},
 {n:'Cégep de Saint-Hyacinthe',s:'omnivox',u:'https://cegepsth.omnivox.ca',a:'cegepsth cegep saint hyacinthe saint-hyacinthe'},
 {n:'Cégep de Saint-Jérôme',s:'omnivox',u:'https://cstj.omnivox.ca',a:'cstj cegep saint jerome saint-jerome'},
 {n:'Cégep de Saint-Laurent',s:'omnivox',u:'https://cegep-st-laurent.omnivox.ca',a:'cegepsl cegep saint laurent cegep-st-laurent saint-laurent'},
 {n:'Cégep de Sainte-Foy',s:'omnivox',u:'https://cegep-ste-foy.omnivox.ca',a:'csfoy cegep sainte foy cegep-ste-foy sainte-foy'},
 {n:'Cégep de Sept-Îles',s:'omnivox',u:'https://cegep-sept-iles.omnivox.ca',a:'cegep-sept-iles cegep sept iles sept-iles'},
 {n:'Cégep de Shawinigan',s:'omnivox',u:'https://cshawi.omnivox.ca',a:'cshawi cegep shawinigan'},
 {n:'Cégep de Sherbrooke',s:'omnivox',u:'https://cegepsherbrooke.omnivox.ca',a:'cegepsherbrooke cegep sherbrooke'},
 {n:'Cégep de Sorel-Tracy',s:'omnivox',u:'https://cegepst.omnivox.ca',a:'cegepst cegep sorel tracy sorel-tracy'},
 {n:'Cégep de Thetford',s:'omnivox',u:'https://cegepth.omnivox.ca',a:'cegepth cegep thetford'},
 {n:'Cégep de Trois-Rivières',s:'omnivox',u:'https://cegeptr.omnivox.ca',a:'cegeptr cegep trois rivieres trois-rivieres'},
 {n:'Cégep de Valleyfield',s:'omnivox',u:'https://colval.omnivox.ca',a:'colval cegep valleyfield'},
 {n:'Cégep de Victoriaville',s:'omnivox',u:'https://cgpvicto.omnivox.ca',a:'cgpvicto cegep victoriaville'},
 {n:'Cégep du Vieux Montréal',s:'omnivox',u:'https://cvm.omnivox.ca',a:'vieux montreal cvm cegep'},
 {n:'Cégep Édouard-Montpetit',s:'omnivox',u:'https://cegepmontpetit.omnivox.ca',a:'edouard-montpetit longueuil cegep edouard montpetit cegepmontpetit'},
 {n:'Cégep Garneau',s:'omnivox',u:'https://cegepgarneau.omnivox.ca',a:'francois-xavier-garneau cegep garneau cegepgarneau'},
 {n:'Cégep Heritage College',s:'omnivox',u:'https://cegep-heritage.omnivox.ca',a:'heritage cegep college cegep-heritage'},
 {n:'Cégep Limoilou',s:'omnivox',u:'https://climoilou.omnivox.ca',a:'limoilou quebec cegep climoilou'},
 {n:'Cégep Marie-Victorin',s:'omnivox',u:'https://collegemv.omnivox.ca',a:'collegemv cegep marie victorin marie-victorin'},
 {n:'Cégep Saint-Jean-sur-Richelieu',s:'omnivox',u:'https://cstjean.omnivox.ca',a:'cegep saint jean sur richelieu cstjean saint-jean'},
 {n:'Champlain College Lennoxville',s:'omnivox',u:'https://champlaincollege-lennoxville.omnivox.ca',a:'champlain lennoxville college champlaincollege-lennoxville'},
 {n:'Champlain College Saint-Lambert',s:'omnivox',u:'https://champlaincollege-st-lambert.omnivox.ca',a:'champlain saint-lambert college saint lambert champlaincollege-st-lambert'},
 {n:'Champlain College Saint-Lawrence',s:'omnivox',u:'https://slc.omnivox.ca',a:'champlain saint-lawrence college saint lawrence slc'},
 {n:'Clemson University',s:'canvas',u:'https://clemson.instructure.com',a:'clemson university'},
 {n:'Collège Ahuntsic',s:'omnivox',u:'https://collegeahuntsic.omnivox.ca',a:'ahuntsic college collegeahuntsic'},
 {n:'Collège André-Grasset',s:'omnivox',u:'https://grasset.omnivox.ca',a:'grasset college andre'},
 {n:'Collège Bart',s:'omnivox',u:'https://bart.omnivox.ca',a:'college bart'},
 {n:"Collège d'Alma",s:'moodle',u:'https://moodle.collegealma.ca',a:'alma college collegealma'},
 {n:'Collège de Maisonneuve',s:'omnivox',u:'https://cmaisonneuve.omnivox.ca',a:'cmaisonneuve college maisonneuve'},
 {n:'Collège de Montréal',s:'moodle',u:'https://cours.college-montreal.qc.ca',a:'cdm college montreal cours de'},
 {n:'Collège Ellis',s:'omnivox',u:'https://ellis.omnivox.ca',a:'college ellis trois-rivieres'},
 {n:'Collège Gérald-Godin',s:'omnivox',u:'https://cgodin.omnivox.ca',a:'cgodin college gerald godin gerald-godin'},
 {n:'Collège Inter-Dec',s:'omnivox',u:'https://interdec.omnivox.ca',a:'college inter dec interdec inter-dec'},
 {n:'Collège Jean-de-Brébeuf',s:'omnivox',u:'https://brebeuf.omnivox.ca',a:'brebeuf college jean'},
 {n:'Collège Laflèche',s:'omnivox',u:'https://clafleche.omnivox.ca',a:'college lafleche clafleche trois-rivieres'},
 {n:'Collège LaSalle',s:'omnivox',u:'https://collegelasalle.omnivox.ca',a:'lasalle college collegelasalle'},
 {n:'Collège Lionel-Groulx',s:'moodle',u:'https://clg.moodle.decclic.qc.ca',a:'clg college lionel groulx'},
 {n:'Collège Marianopolis',s:'omnivox',u:'https://marianopolis.omnivox.ca',a:'marianopolis college'},
 {n:'Collège Mathieu',s:'omnivox',u:'https://collegemathieu.omnivox.ca',a:'college mathieu collegemathieu gravalbourg'},
 {n:'Collège Mérici',s:'omnivox',u:'https://college-merici.omnivox.ca',a:'college merici college-merici'},
 {n:'Collège Montmorency',s:'moodle',u:'https://cmontmorency.moodle.decclic.qc.ca',a:'cmontmorency college montmorency'},
 {n:'College of New Caledonia',s:'brightspace',u:'https://brightspace.cnc.bc.ca',a:'college new caledonia nouvelle-caledonie prince george cnc'},
 {n:'College of the North Atlantic',s:'brightspace',u:'https://d2l.cna.nl.ca',a:'college north atlantic terre-neuve cna'},
 {n:'Collège Regina Assumpta',s:'moodle',u:'https://moodle.reginaassumpta.qc.ca',a:'cra college regina assumpta reginaassumpta'},
 {n:'Collège Sainte-Anne',s:'omnivox',u:'https://sainteanne.omnivox.ca',a:'sainte-anne college sainte anne sainteanne'},
 {n:'Collège Universel',s:'omnivox',u:'https://collegeuniversel.omnivox.ca',a:'college universel collegeuniversel gatineau'},
 {n:'Colorado State University',s:'canvas',u:'https://canvas.colostate.edu',a:'colorado state university colostate'},
 {n:'Columbia University',s:'canvas',u:'https://courseworks2.columbia.edu',a:'columbia university courseworks2'},
 {n:'Cornell University',s:'canvas',u:'https://canvas.cornell.edu',a:'cornell university'},
 {n:'Creighton University',s:'canvas',u:'https://blueline.instructure.com',a:'creighton university blueline'},
 {n:'CY Cergy Paris Université',s:'moodle',u:'https://cours.cyu.fr',a:'cergy cyu cy paris universite cours'},
 {n:'Dartmouth College',s:'canvas',u:'https://canvas.dartmouth.edu',a:'dartmouth college'},
 {n:'Dawson College',s:'omnivox',u:'https://dawsoncollege.omnivox.ca',a:'dawson college dawsoncollege'},
 {n:'Drexel University',s:'canvas',u:'https://canvas.drexel.edu',a:'drexel university'},
 {n:'Duke University',s:'canvas',u:'https://canvas.duke.edu/login/canvas',a:'duke university'},
 {n:'École de technologie supérieure',s:'moodle',u:'https://ena.etsmtl.ca',a:'ets genie ecole technologie superieure ena etsmtl'},
 {n:'École nationale de cirque',s:'omnivox',u:'https://ecolenationaledecirque.omnivox.ca',a:'ecole nationale cirque ecolenationaledecirque'},
 {n:'EM Lyon',s:'moodle',u:'https://learn.em-lyon.com',a:'emlyon em lyon learn'},
 {n:'ENS Paris',s:'moodle',u:'https://moodle.psl.eu',a:'ens paris'},
 {n:'First Nations University of Canada',s:'brightspace',u:'https://learn.fnuniv.ca',a:'first nations university canada learn premieres fnuniv'},
 {n:'Fleming College',s:'brightspace',u:'https://learn.flemingcollege.ca',a:'fleming college learn peterborough flemingcollege'},
 {n:'Florida International University',s:'canvas',u:'https://canvas.fiu.edu',a:'florida international university fiu'},
 {n:'Florida State University',s:'canvas',u:'https://canvas.fsu.edu',a:'florida state university fsu'},
 {n:'George Brown College',s:'brightspace',u:'https://learn.georgebrown.ca',a:'george brown college learn toronto georgebrown'},
 {n:'George Mason University',s:'canvas',u:'https://canvas.gmu.edu/login/canvas',a:'george mason university gmu'},
 {n:'George Washington University',s:'canvas',u:'https://canvas.gwu.edu',a:'george washington university gwu'},
 {n:'Georgetown University',s:'canvas',u:'https://canvas.georgetown.edu',a:'georgetown university'},
 {n:'Georgia Institute of Technology',s:'canvas',u:'https://canvas.gatech.edu',a:'georgia institute technology gatech'},
 {n:'Grenoble École de Management',s:'moodle',u:'https://moodle.grenoble-em.com',a:'grenoble ecole management em'},
 {n:'Grenoble INP',s:'moodle',u:'https://moodle.grenoble-inp.fr',a:'grenoble inp'},
 {n:'Harvard University',s:'canvas',u:'https://canvas.harvard.edu',a:'harvard university'},
 {n:'HEC Montréal',s:'canvas',u:'https://zonecours.hec.ca',a:'hec zonecours gestion montreal'},
 {n:'Howard University',s:'canvas',u:'https://canvas.howard.edu',a:'howard university'},
 {n:'INALCO',s:'moodle',u:'https://moodle.inalco.fr',a:'inalco'},
 {n:'Indiana University Bloomington',s:'canvas',u:'https://canvas.iu.edu',a:'indiana university bloomington'},
 {n:'INRS',s:'moodle',u:'https://moodle.inrs.ca',a:'recherche scientifique inrs'},
 {n:'INSA Lyon',s:'moodle',u:'https://moodle.insa-lyon.fr',a:'insa lyon'},
 {n:'INSA Rouen',s:'moodle',u:'https://moodle.insa-rouen.fr',a:'insa rouen'},
 {n:'INSA Strasbourg',s:'moodle',u:'https://moodle.insa-strasbourg.fr',a:'insa strasbourg'},
 {n:'INSA Toulouse',s:'moodle',u:'https://moodle.insa-toulouse.fr',a:'insa toulouse'},
 {n:"Institut de tourisme et d'hôtellerie du Québec",s:'moodle',u:'https://moodle.ithq.qc.ca',a:'ithq hotellerie tourisme institut quebec'},
 {n:'Institut Polytechnique de Paris',s:'moodle',u:'https://moodle.ip-paris.fr',a:'ip paris institut polytechnique'},
 {n:'Iowa State University',s:'canvas',u:'https://canvas.iastate.edu',a:'iowa state university iastate'},
 {n:'ISAE-SUPAERO',s:'moodle',u:'https://online.isae-supaero.fr',a:'isae supaero online'},
 {n:'John Abbott College',s:'omnivox',u:'https://johnabbott.omnivox.ca',a:'john abbott college johnabbott'},
 {n:'Kansas State University',s:'canvas',u:'https://canvas.k-state.edu',a:'kansas state university k'},
 {n:'Lambton College',s:'brightspace',u:'https://d2l.lambtoncollege.ca',a:'lambton college sarnia lambtoncollege'},
 {n:'Laurentian University',s:'brightspace',u:'https://d2l.laurentian.ca',a:'laurentian university laurentienne sudbury'},
 {n:'Lethbridge Polytechnic',s:'canvas',u:'https://lethbridge.instructure.com',a:'lethbridge polytechnic'},
 {n:'Louisiana State University',s:'moodle',u:'https://moodle.lsu.edu',a:'louisiana state university lsu'},
 {n:'Manitoba Institute of Trades and Technology',s:'brightspace',u:'https://mylearning.mitt.ca',a:'manitoba institute trades technology mylearning mitt winnipeg'},
 {n:'Marquette University',s:'brightspace',u:'https://d2l.mu.edu',a:'marquette university'},
 {n:'Massachusetts Institute of Technology',s:'canvas',u:'https://canvas.mit.edu/login/canvas',a:'mit massachusetts institute technology'},
 {n:'McGill University',s:'brightspace',u:'https://mycourses2.mcgill.ca',a:'mcgill university mycourses2'},
 {n:'McMaster University',s:'brightspace',u:'https://avenue.mcmaster.ca',a:'mcmaster university avenue'},
 {n:'Memorial University of Newfoundland',s:'brightspace',u:'https://online.mun.ca',a:'memorial terre-neuve university newfoundland online mun'},
 {n:'Michigan State University',s:'brightspace',u:'https://d2l.msu.edu',a:'michigan state university msu'},
 {n:'Mohawk College',s:'canvas',u:'https://mycanvas.mohawkcollege.ca',a:'mohawk college mycanvas hamilton'},
 {n:'Mount Saint Vincent University',s:'moodle',u:'https://learn.msvu.ca',a:'mount saint vincent university learn msvu'},
 {n:'NEOMA Business School',s:'moodle',u:'https://online.neoma-bs.fr',a:'neoma business school online bs'},
 {n:'New Brunswick Community College',s:'brightspace',u:'https://elearning.nbcc.ca',a:'new brunswick community college elearning nbcc'},
 {n:'New Jersey Institute of Technology',s:'canvas',u:'https://canvas.njit.edu',a:'new jersey institute technology njit'},
 {n:'Niagara College',s:'brightspace',u:'https://brightspace.niagaracollege.ca',a:'niagara college niagaracollege'},
 {n:'North Carolina State University',s:'canvas',u:'https://canvas.ncsu.edu',a:'north carolina state university ncsu'},
 {n:'North Island College',s:'brightspace',u:'https://mycourses.nic.bc.ca',a:'north island college mycourses nic'},
 {n:'Northeastern University',s:'canvas',u:'https://canvas.northeastern.edu',a:'northeastern university'},
 {n:'Northern Alberta Institute of Technology',s:'brightspace',u:'https://lms.nait.ca',a:'northern alberta institute technology nait edmonton'},
 {n:'Northern Lights College',s:'brightspace',u:'https://d2l.nlc.bc.ca',a:'northern lights college nlc'},
 {n:'Northwestern University',s:'canvas',u:'https://canvas.northwestern.edu',a:'northwestern university'},
 {n:'Nova Scotia Community College',s:'brightspace',u:'https://nscconline.brightspace.com',a:'nova scotia community college nscconline nscc'},
 {n:'Ohio University',s:'canvas',u:'https://canvas.ohio.edu',a:'ohio university'},
 {n:'Oklahoma State University',s:'canvas',u:'https://canvas.okstate.edu',a:'oklahoma state university okstate'},
 {n:'Oregon State University',s:'canvas',u:'https://canvas.oregonstate.edu',a:'oregon state university oregonstate'},
 {n:'Polytechnique Montréal',s:'moodle',u:'https://moodle.polymtl.ca',a:'poly polymtl genie polytechnique montreal'},
 {n:'Portland State University',s:'canvas',u:'https://learn.pdx.edu',a:'portland state university learn pdx'},
 {n:'Purdue University',s:'brightspace',u:'https://purdue.brightspace.com',a:'purdue university'},
 {n:"Queen's University",s:'brightspace',u:'https://onq.queensu.ca',a:'queens kingston queen university onq queensu'},
 {n:'Red River College Polytechnic',s:'brightspace',u:'https://learn.rrc.ca',a:'red river college polytechnic learn winnipeg rrc'},
 {n:'Rochester Institute of Technology',s:'brightspace',u:'https://mycourses.rit.edu',a:'rochester institute technology mycourses rit'},
 {n:'Royal Roads University',s:'moodle',u:'https://moodle.royalroads.ca/moodle',a:'royal roads university royalroads'},
 {n:'Saint Louis University',s:'canvas',u:'https://canvas.slu.edu',a:'saint louis university slu'},
 {n:"Saint Mary's University",s:'brightspace',u:'https://smu.brightspace.com',a:'saint mary university smu halifax'},
 {n:'San Diego State University',s:'canvas',u:'https://canvas.sdsu.edu',a:'san diego state university sdsu'},
 {n:'San Francisco State University',s:'canvas',u:'https://canvas.sfsu.edu',a:'san francisco state university sfsu'},
 {n:'San Jose State University',s:'canvas',u:'https://sjsu.instructure.com',a:'san jose state university sjsu'},
 {n:'Selkirk College',s:'moodle',u:'https://learn.selkirk.ca',a:'selkirk college learn nelson'},
 {n:'Séminaire de Sherbrooke',s:'omnivox',u:'https://seminaire-sherbrooke.omnivox.ca',a:'seminaire sherbrooke seminaire-sherbrooke'},
 {n:'Sheridan College',s:'brightspace',u:'https://slate.sheridancollege.ca',a:'sheridan college sheridancollege'},
 {n:'Simon Fraser University',s:'canvas',u:'https://canvas.sfu.ca',a:'sfu simon fraser university'},
 {n:'SKEMA Business School',s:'moodle',u:'https://moodle.skema.edu',a:'skema business school'},
 {n:'Southern Alberta Institute of Technology',s:'brightspace',u:'https://learn.sait.ca',a:'southern alberta institute technology learn sait calgary'},
 {n:'St. Francis Xavier University',s:'moodle',u:'https://moodle.stfx.ca',a:'st francis xavier university stfx'},
 {n:'St. Thomas University',s:'moodle',u:'https://moodle.stu.ca',a:'st thomas university saint-thomas stu'},
 {n:'Stanford University',s:'canvas',u:'https://canvas.stanford.edu/login/canvas',a:'stanford university'},
 {n:'Stony Brook University',s:'brightspace',u:'https://mycourses.stonybrook.edu',a:'stony brook university mycourses stonybrook'},
 {n:'TÉLUQ',s:'moodle',u:'https://ena.teluq.ca',a:'tele-universite teluq ena'},
 {n:'Texas A&M University',s:'canvas',u:'https://canvas.tamu.edu',a:'texas university tamu'},
 {n:'Texas Christian University',s:'brightspace',u:'https://d2l.tcu.edu',a:'texas christian university tcu'},
 {n:'Thompson Rivers University',s:'moodle',u:'https://moodle.tru.ca',a:'thompson rivers university tru kamloops'},
 {n:'Toulouse INP',s:'moodle',u:'https://moodle.inp-toulouse.fr',a:'toulouse inp'},
 {n:'Trent University',s:'brightspace',u:'https://learn.trentu.ca',a:'trent university learn trentu'},
 {n:'Tufts University',s:'canvas',u:'https://canvas.tufts.edu',a:'tufts university'},
 {n:'Université Claude Bernard Lyon 1',s:'moodle',u:'https://moodle.univ-lyon1.fr',a:'universite claude bernard lyon univ lyon1'},
 {n:'Université Concordia',s:'brightspace',u:'https://brightspace.concordia.ca',a:'concordia universite'},
 {n:"Université d'Aix-Marseille",s:'moodle',u:'https://ametice.univ-amu.fr',a:'amu aix marseille universite ametice univ'},
 {n:"Université d'Angers",s:'moodle',u:'https://moodle.univ-angers.fr',a:'universite angers univ'},
 {n:"Université d'Artois",s:'moodle',u:'https://moodle.univ-artois.fr',a:'universite artois univ'},
 {n:'Université de Bordeaux',s:'moodle',u:'https://moodle.u-bordeaux.fr',a:'universite bordeaux u'},
 {n:'Université de Caen Normandie',s:'moodle',u:'https://ecampus.unicaen.fr',a:'universite caen normandie ecampus unicaen'},
 {n:'Université de Guyane',s:'moodle',u:'https://cours.univ-guyane.fr',a:'universite guyane cours univ'},
 {n:"Université de l'Ontario français",s:'moodle',u:'https://moodle.uontario.ca',a:'universite ontario francais uof uontario'},
 {n:'Université de la Nouvelle-Calédonie',s:'moodle',u:'https://moodle.unc.nc',a:'universite nouvelle caledonie unc'},
 {n:'Université de La Réunion',s:'moodle',u:'https://moodle-central-2026.univ-reunion.fr',a:'universite reunion univ'},
 {n:'Université de Lille',s:'moodle',u:'https://moodle.univ-lille.fr',a:'universite lille univ'},
 {n:'Université de Mayotte',s:'moodle',u:'https://moodle.univ-mayotte.fr',a:'universite mayotte univ'},
 {n:'Université de Montpellier',s:'moodle',u:'https://moodle.umontpellier.fr',a:'universite montpellier umontpellier'},
 {n:'Université de Montréal',s:'moodle',u:'https://studium.umontreal.ca',a:'udem universite montreal studium umontreal'},
 {n:'Université de Perpignan',s:'moodle',u:'https://cours.univ-perp.fr',a:'universite perpignan cours univ perp'},
 {n:'Université de Reims Champagne-Ardenne',s:'moodle',u:'https://cours.univ-reims.fr',a:'universite reims champagne ardenne cours univ'},
 {n:'Université de Sherbrooke',s:'moodle',u:'https://moodle.usherbrooke.ca',a:'udes sherbrooke universite usherbrooke'},
 {n:'Université de Strasbourg',s:'moodle',u:'https://moodle.unistra.fr',a:'universite strasbourg unistra'},
 {n:'Université de Technologie de Belfort-Montbéliard',s:'moodle',u:'https://moodle.utbm.fr',a:'universite technologie belfort montbeliard utbm'},
 {n:'Université de Technologie de Compiègne',s:'moodle',u:'https://moodle.utc.fr',a:'universite technologie compiegne utc'},
 {n:'Université de Technologie de Troyes',s:'moodle',u:'https://moodle.utt.fr',a:'universite technologie troyes utt'},
 {n:"Université du Littoral Côte d'Opale",s:'moodle',u:'https://moodle.univ-littoral.fr',a:'universite littoral cote opale univ'},
 {n:'Université Gustave Eiffel',s:'moodle',u:'https://elearning.univ-eiffel.fr',a:'eiffel universite gustave elearning univ'},
 {n:'Université Jean Moulin Lyon 3',s:'moodle',u:'https://moodle.univ-lyon3.fr',a:'universite jean moulin lyon univ lyon3'},
 {n:'Université Lumière Lyon 2',s:'moodle',u:'https://moodle.univ-lyon2.fr',a:'universite lumiere lyon univ lyon2'},
 {n:'Université Marie-et-Louis-Pasteur',s:'moodle',u:'https://moodle.univ-fcomte.fr',a:'universite marie louis pasteur univ fcomte besancon franche-comte'},
 {n:'Université Paris 8',s:'moodle',u:'https://moodle.univ-paris8.fr',a:'paris8 universite paris univ'},
 {n:'Université Paris Cité',s:'moodle',u:'https://moodle.u-paris.fr',a:'upcite universite paris cite u'},
 {n:'Université Paul-Valéry Montpellier 3',s:'moodle',u:'https://moodle.univ-montp3.fr',a:'universite paul valery montpellier univ montp3'},
 {n:'Université Polytechnique Hauts-de-France',s:'moodle',u:'https://moodle.uphf.fr',a:'universite polytechnique hauts france uphf'},
 {n:'Université Sainte-Anne',s:'moodle',u:'https://moodle.usainteanne.ca',a:"universite sainte anne sainte-anne pointe-de-l'eglise usainteanne"},
 {n:'Université Savoie Mont Blanc',s:'moodle',u:'https://moodle.univ-smb.fr',a:'universite savoie mont blanc univ smb'},
 {n:'University at Albany',s:'brightspace',u:'https://brightspace.albany.edu',a:'university at albany'},
 {n:'University College of the North',s:'brightspace',u:'https://d2l.ucn.ca',a:'university college north nord the pas ucn'},
 {n:'University of Arizona',s:'brightspace',u:'https://d2l.arizona.edu',a:'university arizona'},
 {n:'University of British Columbia',s:'canvas',u:'https://canvas.ubc.ca',a:'ubc university british columbia vancouver'},
 {n:'University of Calgary',s:'brightspace',u:'https://d2l.ucalgary.ca',a:'ucalgary university calgary'},
 {n:'University of California, Berkeley',s:'canvas',u:'https://learn.berkeley.edu',a:'uc berkeley university california learn'},
 {n:'University of California, Davis',s:'canvas',u:'https://canvas.ucdavis.edu',a:'university california davis ucdavis'},
 {n:'University of California, Riverside',s:'canvas',u:'https://canvas.ucr.edu',a:'university california riverside ucr'},
 {n:'University of California, San Diego',s:'canvas',u:'https://canvas.ucsd.edu',a:'university california san diego ucsd'},
 {n:'University of California, Santa Cruz',s:'canvas',u:'https://canvas.ucsc.edu',a:'university california santa cruz ucsc'},
 {n:'University of Central Florida',s:'canvas',u:'https://webcourses.ucf.edu',a:'university central florida webcourses ucf'},
 {n:'University of Chicago',s:'canvas',u:'https://canvas.uchicago.edu/login/canvas',a:'uchicago university chicago'},
 {n:'University of Colorado Boulder',s:'canvas',u:'https://canvas.colorado.edu',a:'university colorado boulder'},
 {n:'University of Denver',s:'canvas',u:'https://canvas.du.edu',a:'university denver'},
 {n:'University of Georgia',s:'brightspace',u:'https://elc.uga.edu',a:'university georgia elc'},
 {n:'University of Guelph',s:'brightspace',u:'https://courselink.uoguelph.ca',a:'guelph university courselink'},
 {n:'University of Illinois Urbana-Champaign',s:'canvas',u:'https://canvas.illinois.edu',a:'university illinois urbana champaign'},
 {n:'University of Iowa',s:'canvas',u:'https://icon.uiowa.edu',a:'university iowa icon uiowa'},
 {n:'University of Kansas',s:'canvas',u:'https://canvas.ku.edu',a:'university kansas'},
 {n:'University of Lethbridge',s:'moodle',u:'https://moodle.uleth.ca',a:'university lethbridge uleth'},
 {n:'University of Maryland',s:'canvas',u:'https://elms.umd.edu',a:'university maryland elms umd'},
 {n:'University of Miami',s:'canvas',u:'https://canvas.miami.edu',a:'university miami'},
 {n:'University of Michigan',s:'canvas',u:'https://canvas.umich.edu',a:'umich university michigan'},
 {n:'University of Missouri',s:'canvas',u:'https://canvas.missouri.edu',a:'university missouri'},
 {n:'University of Nebraska-Lincoln',s:'canvas',u:'https://canvas.unl.edu',a:'university nebraska lincoln unl'},
 {n:'University of Nevada, Reno',s:'canvas',u:'https://canvas.unr.edu',a:'university nevada reno unr'},
 {n:'University of New Hampshire',s:'canvas',u:'https://mycourses.unh.edu',a:'university new hampshire mycourses unh'},
 {n:'University of New Mexico',s:'canvas',u:'https://unm.instructure.com/login/canvas',a:'university new mexico learn unm'},
 {n:'University of North Carolina at Charlotte',s:'canvas',u:'https://canvas.charlotte.edu',a:'university north carolina at charlotte'},
 {n:'University of Northern British Columbia',s:'moodle',u:'https://moodle.unbc.ca',a:'university northern british columbia unbc'},
 {n:'University of Notre Dame',s:'canvas',u:'https://canvas.nd.edu',a:'university notre dame'},
 {n:'University of Oklahoma',s:'canvas',u:'https://canvas.ou.edu',a:'university oklahoma'},
 {n:'University of Oregon',s:'canvas',u:'https://canvas.uoregon.edu',a:'university oregon uoregon'},
 {n:'University of Ottawa',s:'brightspace',u:'https://uottawa.brightspace.com',a:'uottawa university ottawa'},
 {n:'University of Pennsylvania',s:'canvas',u:'https://canvas.upenn.edu',a:'upenn penn university pennsylvania'},
 {n:'University of Prince Edward Island',s:'moodle',u:'https://moodle.upei.ca',a:'upei university prince edward island'},
 {n:'University of Regina',s:'moodle',u:'https://moodle.uregina.ca',a:'university regina uregina'},
 {n:'University of Rhode Island',s:'brightspace',u:'https://brightspace.uri.edu',a:'university rhode island uri'},
 {n:'University of Southern California',s:'brightspace',u:'https://brightspace.usc.edu',a:'usc university southern california'},
 {n:'University of Tennessee',s:'canvas',u:'https://utk.instructure.com',a:'university tennessee utk'},
 {n:'University of Texas at Dallas',s:'canvas',u:'https://elearning.utdallas.edu',a:'university texas at dallas elearning utdallas'},
 {n:'University of the Fraser Valley',s:'brightspace',u:'https://brightspace.ufv.ca',a:'university fraser valley ufv'},
 {n:'University of Toronto',s:'canvas',u:'https://q.utoronto.ca/login/canvas',a:'uoft toronto university q utoronto'},
 {n:'University of Utah',s:'canvas',u:'https://canvas.utah.edu',a:'university utah'},
 {n:'University of Vermont',s:'brightspace',u:'https://brightspace.uvm.edu',a:'university vermont uvm'},
 {n:'University of Washington',s:'canvas',u:'https://canvas.uw.edu',a:'uw udub university washington'},
 {n:'University of Waterloo',s:'brightspace',u:'https://learn.uwaterloo.ca',a:'uwaterloo university waterloo learn'},
 {n:'University of Windsor',s:'brightspace',u:'https://brightspace.uwindsor.ca',a:'university windsor uwindsor'},
 {n:'University of Winnipeg',s:'brightspace',u:'https://nexus.uwinnipeg.ca',a:'university winnipeg nexus'},
 {n:'University of Wisconsin-Madison',s:'canvas',u:'https://canvas.wisc.edu',a:'university wisconsin madison wisc'},
 {n:'UQAC',s:'moodle',u:'https://moodle.uqac.ca',a:'chicoutimi saguenay uqac'},
 {n:'UQAM',s:'moodle',u:'https://ena01.uqam.ca',a:'uqam universite du quebec a montreal'},
 {n:'UQAT',s:'moodle',u:'https://moodle.uqat.ca',a:'abitibi temiscamingue rouyn uqat'},
 {n:'UQO',s:'moodle',u:'https://moodle.uqo.ca',a:'outaouais gatineau uqo'},
 {n:'UQTR',s:'moodle',u:'https://moodle.uqtr.ca',a:'trois-rivieres uqtr'},
 {n:'Vancouver Community College',s:'moodle',u:'https://moodle.vcc.ca',a:'vancouver community college vcc'},
 {n:'Vancouver Island University',s:'brightspace',u:'https://d2l.viu.ca',a:'vancouver island university viu nanaimo'},
 {n:'Vanderbilt University',s:'brightspace',u:'https://brightspace.vanderbilt.edu',a:'vanderbilt university'},
 {n:'Vanier College',s:'omnivox',u:'https://vaniercollege.omnivox.ca',a:'vanier college vaniercollege'},
 {n:'Villanova University',s:'brightspace',u:'https://brightspace.villanova.edu',a:'villanova university'},
 {n:'Virginia Commonwealth University',s:'canvas',u:'https://canvas.vcu.edu',a:'virginia commonwealth university vcu'},
 {n:'Wake Forest University',s:'canvas',u:'https://canvas.wfu.edu',a:'wake forest university wfu'},
 {n:'Washington State University',s:'canvas',u:'https://learn.wsu.edu',a:'washington state university learn wsu'},
 {n:'Washington University in St. Louis',s:'canvas',u:'https://canvas.wustl.edu',a:'washington university in st louis wustl'},
 {n:'Wilfrid Laurier University',s:'brightspace',u:'https://d2l.wlu.ca',a:'wlu laurier wilfrid university'},
 {n:'Worcester Polytechnic Institute',s:'canvas',u:'https://canvas.wpi.edu/login/canvas',a:'worcester polytechnic institute wpi'},
 {n:'Yale University',s:'canvas',u:'https://canvas.yale.edu',a:'yale university'}
];
function schoolKey(text){return String(text).normalize('NFD').replace(/[\u0300-\u036f]/g,'').toLowerCase()}
for(const school of schools)school.k=schoolKey(school.n+' '+school.a);
function schoolMatches(value){const q=schoolKey(value).trim();if(q.length<2)return[];const scored=[];for(const school of schools){const name=schoolKey(school.n),words=school.k.split(' ');let score=name.startsWith(q)?0:words.includes(q)?1:name.split(' ').some(word=>word.startsWith(q))?2:words.some(word=>word.startsWith(q))?3:school.k.includes(q)?4:9;if(score<9)scored.push([score,school])}scored.sort((a,b)=>a[0]-b[0]||a[1].n.localeCompare(b[1].n,'fr'));return scored.slice(0,6).map(hit=>hit[1])}
function selectSchool(school){const account=state.accounts.find(a=>a.service===school.s&&a.url.replace(/\/+$/,'')===school.u.replace(/\/+$/,''));state.schoolIndex=-1;openService(school.s,account?.id||'');if(!account){state.editing=true;state.draft.url=school.u}render()}
function selected(){return state.accounts.find(a=>a.service===state.service&&a.id===state.accountId)}
function messageLang(text){return t(text)===text?'en':language}
function notice(){return state.notice?.error?`<div class="notice error" role="alert" lang="${messageLang(state.notice.text)}">${esc(t(state.notice.text))}</div>`:''}
function esc(s){return String(s??'').replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]))}
function icon(name){const paths={copy:'<rect x="8" y="8" width="13" height="13" rx="2"/><path d="M16 5V4a1 1 0 0 0-1-1H4a1 1 0 0 0-1 1v11a1 1 0 0 0 1 1h1"/>',check:'<path d="m5 12 4 4L19 6"/>',back:'<path d="m12 5-7 7 7 7M5 12h14"/>',refresh:'<path d="M19 8a8 8 0 1 0 1 7M19 3v5h-5"/>',spark:'<path d="M11 3.5l1.6 4.2 4.2 1.6-4.2 1.6L11 15.1l-1.6-4.2L5.2 9.3l4.2-1.6z"/><path d="M17.5 14.5l.9 2.2 2.2.9-2.2.9-.9 2.2-.9-2.2-2.2-.9 2.2-.9z"/>',search:'<circle cx="11" cy="11" r="6.5"/><path d="m16 16 4.5 4.5"/>',down:'<path d="M12 5v14m-5.5-5.5L12 19l5.5-5.5"/>'};return `<svg viewBox="0 0 24 24" aria-hidden="true">${paths[name]||paths.back}</svg>`}
function normalizeUrl(value,service){const raw=(value||'').trim().replace(/\s+/g,'');if(!raw)return raw;const host=raw.replace(/^https?:\/\//i,'').split('/')[0];if(service==='omnivox')return 'https://'+(/omnivox\.ca$/i.test(host)?host:host.includes('.')?host:host+'.omnivox.ca');return 'https://'+raw.replace(/^https?:\/\//i,'').replace(/\/+$/,'')}
function findRows(matches){return matches.length?matches.map((school,index)=>`<button type="button" class="find-row" role="option" id="find-opt-${index}" aria-selected="${index===state.schoolIndex}" data-school="${schools.indexOf(school)}" tabindex="-1"><img src="/assets/${school.s}.png" alt=""><span>${esc(school.n)}</span><span class="find-service">${names[school.s]}</span></button>`).join(''):`<p class="find-empty">${t('No school found')}</p>`}
function findField(){const query=state.draft.school||'';const matches=schoolMatches(query);const open=query.trim().length>=2;return `<div class="find">${icon('search')}<input id="school-find" name="school" type="search" role="combobox" aria-haspopup="listbox" aria-expanded="${open}" aria-controls="find-list" aria-autocomplete="list" aria-label="${t('Search your school')}" placeholder="${t('Search your school')}" autocomplete="off" spellcheck="false" value="${esc(query)}"${open&&state.schoolIndex>=0?` aria-activedescendant="find-opt-${state.schoolIndex}"`:''}>${open?`<div class="find-list" id="find-list" role="listbox" aria-label="${t('Search your school')}">${findRows(matches)}</div>`:''}</div>`}
// Mise à jour en place : re-rendre l'écran déplacerait le curseur au début du champ.
function renderFindList(){const input=$('school-find');if(!input)return;const query=state.draft.school||'';const matches=schoolMatches(query);const find=input.closest('.find');let list=find.querySelector('.find-list');if(query.trim().length<2){list?.remove();input.setAttribute('aria-expanded','false');input.removeAttribute('aria-activedescendant');return}if(!list){list=document.createElement('div');list.className='find-list';list.id='find-list';list.setAttribute('role','listbox');list.setAttribute('aria-label',t('Search your school'));find.appendChild(list)}list.innerHTML=findRows(matches);input.setAttribute('aria-expanded','true');if(state.schoolIndex>=0)input.setAttribute('aria-activedescendant','find-opt-'+state.schoolIndex);else input.removeAttribute('aria-activedescendant')}
function mapLines(){return `<svg class="map-lines" aria-hidden="true"><defs><linearGradient id="map-branch-gradient" gradientUnits="userSpaceOnUse" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#b7c9dc"/><stop offset="1" stop-color="#7899bd"/></linearGradient><marker id="map-arrow" viewBox="0 0 8 8" refX="7" refY="4" markerWidth="5" markerHeight="5" markerUnits="userSpaceOnUse" orient="auto"><path d="M0 0 8 4 0 8Z" fill="#7899bd"/></marker></defs><g class="map-paths"></g><g class="map-ports"></g></svg>`}
function home(){const tiles=['moodle','omnivox','brightspace','canvas'].map(s=>{const ofService=state.accounts.filter(a=>a.service===s),connected=ofService.length>0,enabled=ofService.some(a=>a.enabled!==false);return `<button class="tile ${connected?'connected':''} ${connected&&!enabled?'service-off':''}" data-open="${s}" aria-label="${names[s]} ${connected?'manage connection':'connect'}"><img src="/assets/${s}.png" alt=""><span>${names[s]}</span>${connected?'<i class="dot"></i>':''}</button>`}).join('');const ready=state.tunnel.startsWith('Ready(');const value=ready?state.link:state.tunnel.startsWith('Starting')?t('Creating your public link…'):state.tunnel.startsWith('Reconnecting')?t('Reconnecting your public link…'):state.tunnel.startsWith('Failed')?t('Unavailable'):state.tunnel==='Stopped'?t('Off'):t('Connect a service to create your link');return `<section class="home"><h1 class="sr-only">${t('Student MCP')}</h1>${findField()}<div class="mind-map">${mapLines()}<div class="tiles">${tiles}</div><div class="link-card"><button class="link-open" data-screen="links"><span class="link-title">${t('Your public link')}</span><span class="link-value">${esc(value)}</span></button><button class="icon-button copy-home" data-copy="home" aria-label="${t('Copy link')}" ${ready?'':'disabled'}>${icon('copy')}</button></div><div class="ais"><button class="assistant-entry" data-assistant-choose>${icon('spark')}<span>${t('Set up an assistant')}</span><span class="assistant-entry-arrow" aria-hidden="true">›</span></button></div></div>${notice()}</section>`}
let mindMapFrame=0;
function scheduleMindMap(){cancelAnimationFrame(mindMapFrame);mindMapFrame=requestAnimationFrame(drawMindMap)}
function drawMindMap(){
 const root=document.querySelector('.mind-map');if(!root)return;
 const box=root.getBoundingClientRect(),svg=root.querySelector('.map-lines');if(!box.width||!box.height)return;
 svg.setAttribute('viewBox',`0 0 ${box.width} ${box.height}`);const gradient=svg.querySelector('#map-branch-gradient');gradient.setAttribute('x2',box.width);gradient.setAttribute('y2',box.height);
 const point=(element,side)=>{const r=element.getBoundingClientRect();return {x:r.left-box.left+(side==='left'?0:side==='right'?r.width:r.width/2),y:r.top-box.top+(side==='top'?0:side==='bottom'?r.height:r.height/2)}};
 const link=root.querySelector('.link-card'),linkBox=link.getBoundingClientRect(),compact=matchMedia('(max-width:44rem)').matches;
  const connections=[],tiles=[...root.querySelectorAll('.tile')],assistants=[...root.querySelectorAll('.assistant-entry')];
 tiles.forEach((tile,index)=>{
  const side=compact?'bottom':index<2?'right':'left',source=point(tile,side);
  const target=compact?{x:linkBox.left-box.left+linkBox.width*(index+1)/5,y:linkBox.top-box.top}:{x:linkBox.left-box.left+(index<2?0:linkBox.width),y:linkBox.top-box.top+linkBox.height/2+(index%2===0?-22:22)};
  connections.push({source,target,axis:compact?'y':'x'});
 });
  assistants.forEach(assistant=>connections.push({source:point(link,'bottom'),target:point(assistant,'top'),axis:'y',assistant:true}));
 const paths=[],ports=[];
  for(const {source,target,axis,assistant} of connections){
  let d;if(axis==='x'){const bend=(target.x-source.x)*.46;d=`M${source.x} ${source.y}C${source.x+bend} ${source.y} ${target.x-bend} ${target.y} ${target.x} ${target.y}`}else{const bend=(target.y-source.y)*.46;d=`M${source.x} ${source.y}C${source.x} ${source.y+bend} ${target.x} ${target.y-bend} ${target.x} ${target.y}`}
   paths.push(`<path class="map-halo" d="${d}"/><path class="map-branch" ${assistant?'data-map-assistant ':''}d="${d}" marker-end="url(#map-arrow)"/>`);ports.push(`<circle class="map-port" cx="${source.x}" cy="${source.y}" r="2.5"/>`);
 }
 svg.querySelector('.map-paths').innerHTML=paths.join('');svg.querySelector('.map-ports').innerHTML=ports.join('');
}
const mindMapObserver='ResizeObserver' in window?new ResizeObserver(scheduleMindMap):null;
window.addEventListener('resize',scheduleMindMap,{passive:true});
function pageNav(backScreen,backLabel,tools=false){return `<div class="page-nav"><button class="back" data-screen="${backScreen}" aria-label="${t(backLabel)}" title="${t(backLabel)}">${icon('back')}</button>${tools?`<button class="back tools-entry" data-tools aria-label="${t('View tools')}" title="${t('View tools')}"><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M4 5h3v3H4zM11 6.5h9M4 11h3v3H4zM11 12.5h9M4 17h3v3H4zM11 18.5h9"/></svg><span>${t('View tools')}</span></button>`:''}</div>`}
function header(service){return `<header class="service-head">${pageNav('home','Back to services',true)}<div class="logo-frame"><img src="/assets/${service}.png" alt=""></div><h1>${names[service]}</h1></header>`}
function accountHost(account){try{return new URL(account.url).host}catch{return account.url||names[state.service]}}
function accountChoices(){
 const accounts=state.accounts.filter(a=>a.service===state.service);if(!accounts.length)return '';
 const healthLabel=t(!state.health?'Checking connection…':state.health.error?'Connection failed':'Connected'),dotClass=!state.health?'pending':state.health.error?'offline':'';
 return `<div class="account-choices" role="list" aria-label="${t('Accounts')}">${accounts.map(a=>{
  const active=a.id===state.accountId,label=a.name||accountHost(a),on=a.enabled!==false;
  const dot=active?`<i class="status-dot ${dotClass}" title="${healthLabel}" aria-hidden="true"></i>`:`<i class="status-dot idle" aria-hidden="true"></i>`;
   const name=active&&state.renaming?`<form id="profile-name-form" class="account-row-select inline-name-form">${dot}<input id="f-profile_name" class="inline-account-name profile-label" name="profile_name" aria-label="${t('Account name')}" maxlength="80" value="${esc(state.draft.profile_name??a.name??'')}" ${state.busy?'disabled':''} autocomplete="off"></form>`:`<button type="button" class="account-row-select" data-account="${esc(a.id)}" aria-current="${active?'true':'false'}" aria-label="${esc(label+(active?t(', selected')+' — '+healthLabel:''))}" ${active?`title="${t('Rename account')}"`:''}>${dot}<span class="account-row-title">${a.name?esc(label):esc(label).replace(/\./g,'.<wbr>')}</span></button>`;
  const actions=active?`<div class="profile-actions"><button class="icon-button" data-action="account-info" aria-label="${t('Account information')}" title="${t('Account information')}" aria-expanded="${state.accountInfo===a.id}" aria-controls="account-info"><svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="8.5"/><path d="M12 10.5v6M12 7.5v.25"/></svg></button><button class="icon-button" data-action="check" aria-label="${t('Check connection')}" title="${t('Check connection')}">${icon('refresh')}</button></div>`:'';
  return `<div class="account-row ${active?'active':''}" role="listitem">${name}${actions}<label class="account-enabled"><span class="sr-only">${t('Available to assistants')}</span><input class="switch" type="checkbox" name="service_enabled" data-account-id="${esc(a.id)}" ${on?'checked':''} ${state.busy?'disabled':''}></label></div>`;
 }).join('')}</div>`;
}
function input(name,label,placeholder,type='text',value='',autofill='',help=''){value=state.draft[name]??value;const secret=type==='password';const open=!!state.help[name];
const helpBtn=help?`<button type="button" class="icon-button help" data-help="${name}" aria-label="${open?'Hide':'Show'} help for ${label}" aria-expanded="${open?'true':'false'}" aria-controls="help-${name}">⋯</button>`:'';
const eyeBtn=secret?`<button type="button" class="icon-button eye" data-eye="${name}" aria-label="${state.visible[name]?'Hide':'Show'} ${label}"><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M2 12s3.5-6 10-6 10 6 10 6-3.5 6-10 6S2 12 2 12Z"/><circle cx="12" cy="12" r="2.5"/><path class="eye-slash" d="M4 4 20 20" ${state.visible[name]?'hidden':''}/></svg></button>`:'';
const helpPanel=help&&open?`<div class="help-panel" id="help-${name}" role="note">${help}</div>`:'';
return `<div class="field"><div class="field-head"><label for="f-${name}">${label}</label>${helpBtn}${eyeBtn}</div>${helpPanel}<input id="f-${name}" class="input" name="${name}" type="${secret&&state.visible[name]?'text':type==='url'?'text':type}" placeholder="${esc(placeholder)}" value="${esc(value)}" autocomplete="${autofill||'off'}" ${type==='url'?'inputmode="url"':''} ${name==='code'?'inputmode="numeric" maxlength="6"':name==='profile_name'?'maxlength="80"':''}></div>`}
function brightspaceReady(v,a){return !!(v.url||a?.url)&&!!(v.token||'').trim()}
function loginScreenContent(){const a=selected(),s=state.service;if(!state.manualLogin)return `<form id="browser-connect-form">${input('url',s==='omnivox'?'Omnivox address':'Site address',s==='omnivox'?'https://cegepxxx.omnivox.ca':s==='moodle'?'https://studium.umontreal.ca':s==='canvas'?'https://zonecours2.hec.ca':'https://school.brightspace.com','url',a?.url||'')}${notice()}<button class="full-button primary" type="submit" ${state.busy?'disabled':''}>Connect with browser</button>${s==='canvas'?'':`<button class="full-button secondary" type="button" data-action="manual-login">Manual sign-in</button>`}</form>`;let fields='',ready=false;if(s==='moodle'){fields=input('url','Site address','https://studium.umontreal.ca','url',a?.url||'')+input('username','Username','firstname.lastname','text','','username')+input('password','Password','Moodle password','password','','current-password');ready=!!state.draft.username&&!!state.draft.password}
if(s==='omnivox'){fields=input('url','Omnivox address','https://cegepxxx.omnivox.ca','url',a?.url||'')+input('username','Student number','0000000','text','','username')+input('password','Password','Omnivox password','password','','current-password');ready=!!(state.draft.url||a?.url)&&!!state.draft.username&&!!state.draft.password}
if(s==='brightspace'){fields=input('url','Site address','https://mycourses2.mcgill.ca','url',a?.url||'')+input('token','Session cookie or access token','d2lSessionVal=…; d2lSecureSessionVal=…','password');ready=brightspaceReady(state.draft,a)}
return `<form id="connect-form">${fields}${notice()}${state.busy?'<p class="busy">Checking the connection…</p>':''}<button class="full-button primary" ${!ready||state.busy?'disabled':''}>${state.busy?'Connecting…':a?'Save changes':`Connect to ${names[s]}`}</button></form>`}
function profileNameForm(){const a=selected();if(!a||state.accountInfo!==a.id)return '';return `<section id="account-info" class="account-info" aria-label="${t('Account information')}"><dl><div><dt>${t('Site address')}</dt><dd class="profile-label">${esc(a.url)}</dd></div><div><dt>${t('Connection')}</dt><dd>${t(!state.health?'Checking connection…':state.health.error?'Connection failed':'Connected')}</dd></div><div><dt>${t('Available to assistants')}</dt><dd>${t(a.enabled!==false?'On':'Off')}</dd></div></dl></section>`}
function manageScreenContent(){return `<div class="card manage-card">${accountChoices()}${profileNameForm()}${state.health?.error?`<p class="hint" lang="${messageLang(state.health.text)}">${esc(t(state.health.text))}</p>`:''}<button class="account-row add-account" data-add><span class="plus" aria-hidden="true">+</span><span>${t('Add account')}</span></button><div class="manage-actions"><button class="secondary" data-action="${state.confirmDisconnect?'cancel-disconnect':'edit'}">${state.confirmDisconnect?t('Cancel'):t('Edit sign-in')}</button><button class="danger" data-action="disconnect">${t('Disconnect')}</button></div></div>${notice()}`}
 function browserProgress(){const p=state.browserLogin?.progress;const labels={preparing:'Preparing the sign-in browser…',downloading:'Downloading Chrome for Testing…',extracting:'Installing the sign-in browser…',opening:'Opening the sign-in window…'};let text=t(labels[p?.phase]||'Finish signing in; this page will update automatically.');if(p?.phase==='downloading'){const mb=n=>(n/1048576).toFixed(1);text+=' '+mb(p.downloaded||0)+(p.total?' / '+mb(p.total):'')+' MB'}return esc(text)}
function serviceScreen(){const a=selected(),waiting=state.browserLogin?.state==='waiting'&&state.browserLogin.service===state.service;return `${header(state.service)}<section class="form-screen">${state.browserDownload?`<p>${t('No compatible browser found. Download Chrome for Testing from Google (about 200 MB)? Your default browser stays the same.')}</p><button class="full-button primary" data-action="download-browser">Download and continue</button><button class="full-button secondary" data-action="decline-browser">Not now</button>`:waiting?`<p role="status">${browserProgress()}</p><button class="full-button secondary" data-action="cancel">Cancel</button>`:state.verification?verifyScreenContent():a&&!state.editing?manageScreenContent():loginScreenContent()}</section>`}
function verifyScreenContent(){return `<h2>Identity verification</h2><p class="subtitle" data-server-text lang="${messageLang(state.verification)}">${esc(t(state.verification))}</p><form id="verify-form">${input('code','Security code (6 digits)','000000','text','','one-time-code')}${notice()}<button class="full-button primary" ${state.busy?'disabled':''}>${state.busy?'Verifying…':'Verify and connect'}</button></form><button class="full-button secondary" data-action="resend" ${state.busy?'disabled':''}>Send a new code</button><button class="full-button secondary" data-action="cancel" ${state.busy?'disabled':''}>Cancel verification</button>`}
function localCard(){const live=state.local&&!!state.localLink;return `<div class="card" data-link-card="local"><div class="status-row">${live?`<span><i class="status-dot"></i>${t('Active')}</span><button class="copy-pill" data-copy="local">${icon('copy')}<span>${t('Copy')}</span></button>`:`<span class="node-state"><i class="node-dot idle"></i>${t('Off')}</span>`}</div>${live?`<div class="link-detail">${esc(state.localLink)}</div>`:''}</div>`}
function publicCard(){const ready=state.tunnel.startsWith('Ready(');const dot=state.tunnel.startsWith('Failed')?'bad':state.tunnel==='Stopped'?'':'wait';return `<div class="card" data-link-card="public"><div class="status-row">${ready?`<span><i class="status-dot"></i>${t('Active')}</span><button class="copy-pill" data-copy="public">${icon('copy')}<span>${t('Copy')}</span></button>`:`<span class="node-state"><i class="node-dot ${dot}"></i>${esc(t(publicMessage()))}</span>`}</div>${ready?`<div class="link-detail">${esc(state.link)}</div>`:''}</div>`}
function linkScreen(){return `
 ${pageNav('home','Back to services')}
  <header class="links-heading"><h1>${t('Your links')}</h1></header>
  <section class="link-settings" aria-label="${t('Your links')}">
  <div class="link-setting"><label class="section-label">${t('On this computer')}<input class="switch" type="checkbox" name="local" ${state.local?'checked':''}></label>${localCard()}</div>
  <div class="link-setting"><label class="section-label">${t('From the web')}<input class="switch" type="checkbox" name="public" ${state.public?'checked':''}></label>${publicCard()}</div>
 </section>
 ${notice()}`}
 function publicMessage(){if(!state.public||state.tunnel==='Stopped')return 'Off';if(state.tunnel.startsWith('Starting')||state.tunnel.startsWith('Reconnecting'))return 'Connecting…';if(state.tunnel.startsWith('Failed'))return 'Unavailable';return 'Off'}
let updateState={},updateSubmitting=false,updateActionError='';
function renderUpdate(){
 const u=updateState,button=$('update-button'),status=$('update-status'),text=$('update-label');
 const showButton=!!(u.available||u.installing||u.ready);
 button.hidden=!showButton;text.hidden=showButton;
 text.textContent=u.current||'…';
 text.setAttribute('aria-label',(u.current?'Version '+u.current:'Checking version')+' · '+t(u.error?'Update check failed':u.checked&&!u.checking?'Up to date':'Checking for updates…'));
 const label=t('Install update')+(u.latest?' · '+u.latest:'');
 button.title=label;button.setAttribute('aria-label',label);
 button.disabled=updateSubmitting||!!u.installing||!!u.ready;
 button.setAttribute('aria-busy',String(updateSubmitting||!!u.installing));
 const message=updateActionError||u.error||(u.ready?t('Update ready. student-mcp will restart or open its installer.'):u.installing||updateSubmitting?t('Downloading update…'):'');
 status.hidden=!message;status.textContent=message?t(message):'';
}
$('update-button').addEventListener('click',async()=>{
 if(updateSubmitting||updateState.installing||updateState.ready)return;
 updateSubmitting=true;updateActionError='';renderUpdate();
 try{await api('/action',{action:'update'});updateState=await api('/update')}
 catch(e){updateActionError=e.message}
 finally{updateSubmitting=false;renderUpdate()}
});
 let renderedScreen='',refreshing=false,linksPending=0,linksVersion=0,linksQueue=Promise.resolve(),confirmTimer=null;
function render(){
 renderUpdate();
 const startup=$('app-startup'),startupSwitch=startup.querySelector('input');startup.hidden=false;$('app-startup-label').textContent=t('Launch at startup');startupSwitch.checked=state.autostart;startupSwitch.disabled=state.busy;sizeLanguageMenu();
 const app=document.querySelector('.app');app.classList.toggle('tools-view',state.screen==='tools');app.classList.toggle('home-view',state.screen==='home');app.classList.toggle('assistant-view',state.screen==='assistant');
 const screen=$('screen'),route=state.screen+':'+state.service+':'+state.assistant;
 const active=document.activeElement;
  const focus=active&&screen.contains(active)?{id:active.id,attr:['data-screen','data-copy','data-action','data-eye','data-help','data-open','data-account','data-add','data-assistant','data-guide-step','data-assistant-mode','data-assistant-scan','data-local-assistant','data-assistant-confirm','data-assistant-cancel','name'].find(a=>active.hasAttribute(a)),value:null}:null;
 if(focus?.attr)focus.value=active.getAttribute(focus.attr);
  screen.innerHTML=state.screen==='home'?home():state.screen==='links'?linkScreen():state.screen==='tools'?toolsScreen():state.screen==='assistant'?assistantScreen():serviceScreen();
  localize(screen);
 mindMapObserver?.disconnect();const mindMap=screen.querySelector('.mind-map');if(mindMap){mindMapObserver?.observe(mindMap);scheduleMindMap()}
 if(route!==renderedScreen){const heading=screen.querySelector('h1');if(heading){heading.tabIndex=-1;heading.focus({preventScroll:true})}window.scrollTo(0,0)}
 else if(focus){const target=focus.id?document.getElementById(focus.id):focus.attr?[...screen.querySelectorAll('['+focus.attr+']')].find(el=>el.getAttribute(focus.attr)===focus.value):null;target?.focus({preventScroll:true})}
 renderedScreen=route;
}
async function refresh(background=false){
  if(refreshing)return;refreshing=true;const linkRequestVersion=linksVersion;
 try{
  const d=await api('/state');
  updateState=await api('/update');renderUpdate();
  // Polling may update status, but must never replace an active form or pending settings.
  const changed=JSON.stringify([state.accounts,state.link,state.localLink,state.tunnel,state.autostart])!==JSON.stringify([d.accounts,d.link,d.local_link,d.tunnel,d.autostart]);
   state.accounts=d.accounts;state.link=d.link;state.localLink=d.local_link;if(!linksPending&&linkRequestVersion===linksVersion)state.tunnel=d.tunnel;
  const browserChanged=JSON.stringify(state.browserLogin)!==JSON.stringify(d.browser_login||null);
  state.browserLogin=d.browser_login||null;
  if(browserChanged&&state.browserLogin?.service===state.service){
    if(state.browserLogin.state==='connected'){state.accountId=state.browserLogin.id;state.editing=false;state.draft={};state.notice=null;state.health={text:'Connected'}}
   if(state.browserLogin.state==='failed'){if(state.browserLogin.origin)state.draft.url=state.browserLogin.origin;state.notice={text:state.browserLogin.error,error:true};state.editing=true}
   if(state.screen==='service')render();
  }
  state.autostart=!!d.autostart;
   if((!background||state.screen!=='links')&&!linksPending&&linkRequestVersion===linksVersion){state.local=d.local;state.public=d.public}
  if(!background){state.verification=d.verification;render()}
   else if(changed&&!linksPending&&(state.screen==='home'||state.screen==='links'||state.screen==='assistant')&&document.activeElement?.id!=='school-find')render();
  }catch(e){if(!background){state.notice={text:e.message,error:true};render()}else $('live').textContent=t('Connection status unavailable. Retrying…')}
 finally{refreshing=false}
}
async function checkSelected(){
 const service=state.service,id=state.accountId;
 try{const r=await api('/action',{action:'check',service,id});if(state.service===service&&state.accountId===id){state.health={text:r.message};if(state.screen==='service'&&!state.editing)render()}}
 catch(e){if(state.service===service&&state.accountId===id){state.health={text:e.message,error:true};if(state.screen==='service'&&!state.editing)render()}}
}
function openService(service,accountId){state.service=service;state.screen='service';state.confirmDisconnect=false;state.manualLogin=false;state.otherSchool=false;state.editing=false;state.renaming=false;state.draft={};state.visible={};state.help={};state.notice=null;state.health=null;const a=state.accounts.find(x=>x.service===service&&(accountId===undefined||x.id===accountId));state.accountId=a?.id||'';if(service==='moodle'&&a&&a.url!=='https://moodle.polymtl.ca')state.otherSchool=true;if(a&&state.browserLogin?.state!=='waiting')checkSelected();render()}
async function send(data){if(state.busy)return;state.busy=true;render();try{const r=await api('/action',data);state.notice=r.message?{text:r.message}:null;if(r.browser_download_required){state.browserDownload={...data};return}if(r.browser_login){state.browserLogin=r.browser_login;state.screen='service'}if(r.verification){state.verification=r.verification;state.screen='service'}else if(['connect','verify','cancel','disconnect'].includes(data.action)){state.verification=null;if(data.action==='verify'){state.screen='home';state.editing=false}else if(data.action==='cancel'){state.browserLogin=null;state.screen='service'}else state.screen='home'}if(data.action==='connect')state.editing=false;if(data.action==='rename_account'){state.renaming=false;delete state.draft.profile_name;state.accounts=state.accounts.map(x=>x.service===data.service&&x.id===data.id?{...x,name:r.name??null}:x);$('live').textContent=t('Account name saved')}if(r.stop)state.notice={text:'student-mcp stopped'};await refresh()}catch(e){if(data.action==='autostart')state.autostart=!data.enabled;state.notice={text:e.message,error:true}}finally{state.busy=false;render();if(data.action==='rename_account'&&!state.renaming)document.querySelector('[data-action="rename"]')?.focus()}}
document.addEventListener('click',async e=>{
  const shot=e.target.closest('[data-lightbox]');
  if(shot){
   const lightbox=$('lightbox'),image=$('lightbox-image');
   image.src=shot.dataset.lightbox;image.alt=shot.querySelector('img')?.alt||t('Screenshot');
   lightbox.showModal();$('lightbox').dataset.trigger=shot.dataset.lightbox;document.body.classList.add('lightbox-open');$('lightbox').querySelector('[data-lightbox-close]').focus();return
  }
  if(e.target.closest('[data-lightbox-close]')||e.target.id==='lightbox'){$('lightbox').close();return}
  if(e.target.closest('[data-assistant-choose]')){openAssistant('chatgpt');return}
  const guideStep=e.target.closest('[data-guide-step]');if(guideStep&&!state.assistantBusy){assistantJourney.step=Number(guideStep.dataset.guideStep);state.assistantConfirm=false;render();return}
  if(e.target.closest('[data-guide-next],[data-guide-previous]')&&!state.assistantBusy){assistantJourney.step=Math.max(0,Math.min(2,assistantJourney.step+(e.target.closest('[data-guide-next]')?1:-1)));state.assistantConfirm=false;render();$('setup-title')?.focus({preventScroll:true});return}
 const assistant=e.target.closest('[data-assistant]');if(assistant){openAssistant(assistant.dataset.assistant);return}
  const assistantMode=e.target.closest('[data-assistant-mode]');if(assistantMode&&!state.assistantBusy){localAssistants.mode=assistantMode.dataset.assistantMode;state.assistantRemoveConfirm=false;state.notice=null;render();if(localAssistants.mode==='local'&&!localAssistants.items)await scanAssistants();return}
  if(e.target.closest('[data-assistant-scan]')){await scanAssistants();return}
  const localApp=e.target.closest('[data-local-assistant]');if(localApp&&!state.assistantBusy&&!localAssistants.scanning){localAssistants.selected=localApp.dataset.localAssistant;state.assistantStatus=localAssistants.items.find(app=>app.assistant===localAssistants.selected);state.assistantConfirm=true;state.assistantRemoveConfirm=false;state.notice=null;render();document.querySelector('[data-assistant-confirm],.assistant-local .local-close')?.focus();return}
  const assistantConfirm=e.target.closest('[data-assistant-confirm]');if(assistantConfirm&&!state.assistantBusy){await configureAssistant(assistantConfirm.dataset.assistantConfirm);return}
  const assistantRemove=e.target.closest('[data-assistant-remove]');if(assistantRemove&&!state.assistantBusy){state.assistantRemoveConfirm=true;state.notice=null;render();return}
  if(e.target.closest('[data-assistant-remove-cancel]')&&!state.assistantBusy){state.assistantRemoveConfirm=false;render();return}
  const assistantRemoveConfirm=e.target.closest('[data-assistant-remove-confirm]');if(assistantRemoveConfirm&&!state.assistantBusy){await removeAssistant(assistantRemoveConfirm.dataset.assistantRemoveConfirm);return}
  if(e.target.closest('[data-assistant-cancel]')&&!state.assistantBusy){const id=localAssistants.selected;state.assistantConfirm=false;state.assistantRemoveConfirm=false;localAssistants.selected=null;render();document.querySelector(`[data-local-assistant="${id}"]`)?.focus({preventScroll:true});return}
 if(e.target.closest('[data-tools]')){state.screen='tools';render();loadTools(state.service);return}
 if(e.target.closest('[data-tools-retry]')){loadTools(state.service);return}
 const schoolRow=e.target.closest('[data-school]');if(schoolRow){selectSchool(schools[+schoolRow.dataset.school]);return}
 const copy=e.target.closest('[data-copy]');
 if(copy){
  e.stopPropagation();const local=copy.dataset.copy==='local',url=local?state.localLink:state.link;
  if(!url||(local?!state.local:!state.tunnel.startsWith('Ready(')))return;
   try{
    await navigator.clipboard.writeText(url);$('live').textContent=t('Copied');
    const original=copy.innerHTML,aria=copy.getAttribute('aria-label');
    copy.innerHTML=icon('check')+(copy.classList.contains('copy-pill')?'<span>'+t('Copied')+'</span>':'');copy.classList.add('copy-confirmed');copy.setAttribute('aria-label',t('Copied'));
    if(!matchMedia('(prefers-reduced-motion: reduce)').matches)copy.querySelector('svg').animate([{opacity:0,transform:'scale(.8)'},{opacity:1,transform:'scale(1)'}],{duration:180,easing:'cubic-bezier(0.23, 1, 0.32, 1)'});
    setTimeout(()=>{if(!copy.isConnected)return;copy.innerHTML=original;copy.classList.remove('copy-confirmed');if(aria)copy.setAttribute('aria-label',aria);else copy.removeAttribute('aria-label')},1300);
   }
  catch{state.notice={text:'Could not copy the link. Open link settings and select the link to copy it manually.',error:true}}
   if(state.notice?.error)render();return;
 }
 const screen=e.target.closest('[data-screen]');
 if(screen){if(screen.dataset.screen==='home'&&state.verification){send({action:'cancel'});state.verification=null;}state.screen=screen.dataset.screen;state.notice=null;render();return}
 const opener=e.target.closest('[data-open]');if(opener){openService(opener.dataset.open);return}
  const account=e.target.closest('[data-account]');if(account){if(account.dataset.account===state.accountId&&!state.editing){if(!state.busy){state.renaming=true;state.draft.profile_name=selected()?.name||'';state.notice=null;render();$('f-profile_name')?.focus();$('f-profile_name')?.select()}return}state.accountId=account.dataset.account;state.accountInfo=null;state.confirmDisconnect=false;state.editing=false;state.renaming=false;state.draft={};state.visible={};state.help={};state.health=null;state.notice=null;state.otherSchool=state.service==='moodle'&&!!selected()&&selected().url!=='https://moodle.polymtl.ca';checkSelected();render();return}
 if(e.target.closest('[data-add]')){state.accountId='';state.confirmDisconnect=false;state.editing=true;state.renaming=false;state.manualLogin=false;state.draft={};state.visible={};state.help={};state.health=null;state.otherSchool=false;render();return}
 const eye=e.target.closest('[data-eye]');
  if(eye){const name=eye.dataset.eye;state.visible[name]=!state.visible[name];const field=$('f-'+name);field.type=state.visible[name]?'text':'password';eye.querySelector('.eye-slash').toggleAttribute('hidden',state.visible[name]);eye.setAttribute('aria-label',t((state.visible[name]?'Hide ':'Show ')+field.labels[0].textContent));return}
 const help=e.target.closest('[data-help]');
 if(help){const name=help.dataset.help;state.help[name]=!state.help[name];render();return}
 const action=e.target.closest('[data-action]')?.dataset.action;
 if(action&&action!=='disconnect')state.confirmDisconnect=false;
  if(action==='download-browser'){const data=state.browserDownload;state.browserDownload=null;if(data)send({...data,allow_browser_download:true})}
  else if(action==='decline-browser'){state.browserDownload=null;render()}
  else if(action==='manual-login'){state.manualLogin=true;render()}
 else if(action==='edit'){state.editing=true;state.renaming=false;state.manualLogin=false;state.draft={};state.visible={};state.help={};render()}
 else if(action==='cancel-disconnect'){state.confirmDisconnect=false;render()}
   else if(action==='account-info'){state.accountInfo=state.accountInfo===state.accountId?null:state.accountId;render()}
  else if(action==='cancel-rename'){state.renaming=false;delete state.draft.profile_name;state.notice=null;render();document.querySelector('[data-action="rename"]')?.focus()}
  else if(action==='check'){if(selected())checkSelected()}
 else if(action==='disconnect'){const a=selected();if(!a)return;if(!state.confirmDisconnect){state.confirmDisconnect=true;render();$('live').textContent=t('Click again to disconnect this account.');clearTimeout(confirmTimer);confirmTimer=setTimeout(()=>{state.confirmDisconnect=false;if(state.screen==='service'&&!state.editing)render()},4000);return}clearTimeout(confirmTimer);state.confirmDisconnect=false;send({action,service:state.service,id:a.id})}
 else if(action==='resend'||action==='cancel'){send({action})}
 else if(action==='apply-links'){const local=document.querySelector('[name=local]').checked,publicOn=document.querySelector('[name=public]').checked;send({action:'links',local:local||publicOn,public:publicOn})}
 else if(action==='stop')send({action:'stop'});
});
function connectReady(form){const v=Object.fromEntries(new FormData(form)),a=selected();if(state.service==='moodle')return !!v.username&&!!v.password;if(state.service==='omnivox')return !!(v.url||a?.url)&&!!v.username&&!!v.password;if(state.service==='brightspace')return brightspaceReady(v,a);return !!(v.url||a?.url)&&(!!v.token||!!v.username&&!!v.password)}
document.addEventListener('submit',e=>{if(e.target.id!=='profile-name-form')return;e.preventDefault();send({action:'rename_account',service:state.service,id:state.accountId,name:e.target.elements.profile_name.value}).then(()=>{if(state.screen==='service')document.querySelector(state.renaming?'#f-profile_name':'.account-row.active [data-account]')?.focus()})});
document.addEventListener('keydown',e=>{if(e.target.id==='f-profile_name'&&e.key==='Escape'){e.preventDefault();state.renaming=false;delete state.draft.profile_name;render();document.querySelector('.account-row.active [data-account]')?.focus()}});
document.addEventListener('focusout',e=>{if(e.target.id!=='f-profile_name'||!state.renaming||state.busy||!e.target.isConnected)return;const name=e.target.value;if(name===(selected()?.name||'')){state.renaming=false;delete state.draft.profile_name;render();return}send({action:'rename_account',service:state.service,id:state.accountId,name})});
  document.addEventListener('change',e=>{if(e.target.name==='local'||e.target.name==='public'){const local=e.target.name==='local'?e.target.checked:state.local,publicOn=e.target.name==='public'?e.target.checked:state.public,version=++linksVersion;state.local=local;state.public=publicOn;if(e.target.name==='public')state.tunnel=publicOn?'Starting':'Stopped';linksPending++;const card=document.querySelector('[data-link-card="'+e.target.name+'"]');if(card){card.outerHTML=e.target.name==='local'?localCard():publicCard();localize($('screen'))}else render();linksQueue=linksQueue.catch(()=>{}).then(()=>api('/action',{action:'links',local,public:publicOn})).then(async()=>{if(version===linksVersion){linksPending=0;await refresh()}},async err=>{if(version===linksVersion){linksPending=0;state.notice={text:err.message,error:true};await refresh();render()}})}else if(e.target.name==='autostart'){state.autostart=e.target.checked;send({action:'autostart',enabled:state.autostart})}else if(e.target.name==='service_enabled'){const id=e.target.dataset.accountId||selected()?.id;if(id){const account=state.accounts.find(a=>a.service===state.service&&a.id===id);if(account)account.enabled=e.target.checked;send({action:'service_enabled',service:state.service,id,enabled:e.target.checked})}}});
document.addEventListener('input',e=>{if(e.target.name)state.draft[e.target.name]=e.target.value;if(e.target.name==='school'){state.schoolIndex=-1;renderFindList();return}if(e.target.form?.id==='connect-form')e.target.form.querySelector('button[type="submit"],button:not([type])').disabled=!connectReady(e.target.form)});
document.addEventListener('keydown',e=>{
 const input=e.target.closest&&e.target.closest('#school-find');if(!input)return;
 const matches=schoolMatches(input.value);
 if(e.key==='ArrowDown'||e.key==='ArrowUp'){if(!matches.length)return;e.preventDefault();state.schoolIndex=(state.schoolIndex+(e.key==='ArrowDown'?1:matches.length-1))%matches.length;renderFindList();return}
 if(e.key==='Enter'){e.preventDefault();const school=matches[state.schoolIndex]||matches[0];if(school)selectSchool(school);return}
 if(e.key==='Escape'&&state.draft.school){e.preventDefault();state.draft.school='';state.schoolIndex=-1;input.value='';renderFindList()}
});
document.addEventListener('change',e=>{if(e.target.id==='account-picker'){state.accountId=e.target.value==='__new__'?'':e.target.value;state.editing=e.target.value==='__new__';state.draft={};state.visible={};state.help={};state.health=null;state.verification=null;state.otherSchool=state.service==='moodle'&&!!selected()&&selected().url!=='https://moodle.polymtl.ca';if(selected())checkSelected();render()}if(e.target.name==='other_school'){state.otherSchool=e.target.checked;render()}});
 document.addEventListener('submit',e=>{if(e.target.id==='browser-connect-form'){e.preventDefault();const d=Object.fromEntries(new FormData(e.target));send({action:'browser_connect',service:state.service,id:state.accountId||'',url:normalizeUrl(d.url||selected()?.url||'',state.service)})}else if(e.target.id==='connect-form'){e.preventDefault();const form=e.target,d=Object.fromEntries(new FormData(form)),a=selected();if(!connectReady(form))return;d.service=state.service;d.action='connect';d.id=state.accountId||'';d.url=normalizeUrl(d.url||a?.url||'',state.service)||(state.service==='moodle'?'https://moodle.polymtl.ca':'');for(const name of ['password','token'])state.draft[name]='';send(d)}else if(e.target.id==='verify-form'){e.preventDefault();const code=e.target.elements.code.value;state.draft.code='';send({action:'verify',code})}});
refresh();const timer=setInterval(()=>{if(!document.hidden&&!state.busy)refresh(true)},1500);
