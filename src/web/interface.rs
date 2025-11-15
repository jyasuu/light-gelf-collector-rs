/// Returns the HTML content for the web interface
pub fn get_web_interface() -> String {
    r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>GELF Log Viewer</title>
    <style>
        * {
            box-sizing: border-box;
            margin: 0;
            padding: 0;
        }
        
        body {
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
            background-color: #1a1a1a;
            color: #e0e0e0;
            line-height: 1.6;
        }
        
        .header {
            background: linear-gradient(135deg, #2d3748, #4a5568);
            padding: 1rem 2rem;
            border-bottom: 3px solid #4299e1;
            box-shadow: 0 2px 10px rgba(0,0,0,0.3);
        }
        
        .header h1 {
            color: #63b3ed;
            font-size: 1.8rem;
            font-weight: 600;
        }
        
        .stats {
            color: #a0aec0;
            font-size: 0.9rem;
            margin-top: 0.5rem;
        }
        
        .controls {
            padding: 1rem 2rem;
            background: #2d3748;
            border-bottom: 1px solid #4a5568;
            display: flex;
            gap: 1rem;
            align-items: center;
            flex-wrap: wrap;
        }
        
        .btn {
            background: #4299e1;
            color: white;
            border: none;
            padding: 0.5rem 1rem;
            border-radius: 6px;
            cursor: pointer;
            font-size: 0.9rem;
            font-weight: 500;
            transition: all 0.2s;
        }
        
        .btn:hover {
            background: #3182ce;
            transform: translateY(-1px);
        }
        
        .btn:active {
            transform: translateY(0);
        }
        
        .btn.danger {
            background: #e53e3e;
        }
        
        .btn.danger:hover {
            background: #c53030;
        }
        
        .status {
            padding: 0.5rem 1rem;
            border-radius: 6px;
            font-size: 0.85rem;
            font-weight: 500;
        }
        
        .status.connected {
            background: #38a169;
            color: white;
        }
        
        .status.disconnected {
            background: #e53e3e;
            color: white;
        }
        
        .main-content {
            height: calc(100vh - 140px);
            overflow: hidden;
        }
        
        .log-container {
            height: 100%;
            overflow-y: auto;
            padding: 1rem;
            background: #1a1a1a;
        }
        
        .log-entry {
            background: #2d3748;
            border: 1px solid #4a5568;
            border-radius: 8px;
            margin-bottom: 0.75rem;
            padding: 1rem;
            transition: all 0.2s;
            animation: slideIn 0.3s ease-out;
        }
        
        @keyframes slideIn {
            from {
                opacity: 0;
                transform: translateX(-20px);
            }
            to {
                opacity: 1;
                transform: translateX(0);
            }
        }
        
        .log-entry:hover {
            background: #374151;
            border-color: #63b3ed;
            transform: translateY(-1px);
            box-shadow: 0 4px 12px rgba(0,0,0,0.3);
        }
        
        .log-header {
            display: flex;
            justify-content: space-between;
            align-items: center;
            margin-bottom: 0.5rem;
            flex-wrap: wrap;
            gap: 0.5rem;
        }
        
        .log-level {
            padding: 0.2rem 0.6rem;
            border-radius: 4px;
            font-size: 0.75rem;
            font-weight: 600;
            text-transform: uppercase;
        }
        
        .level-0, .level-1, .level-2, .level-3 { background: #e53e3e; color: white; }
        .level-4 { background: #ed8936; color: white; }
        .level-5 { background: #ecc94b; color: #1a1a1a; }
        .level-6 { background: #48bb78; color: white; }
        .level-7 { background: #4299e1; color: white; }
        
        .timestamp {
            color: #a0aec0;
            font-size: 0.8rem;
            font-family: 'Courier New', monospace;
        }
        
        .host {
            color: #63b3ed;
            font-weight: 500;
            font-size: 0.9rem;
        }
        
        .message {
            margin-top: 0.5rem;
        }
        
        .short-message {
            color: #f7fafc;
            font-weight: 500;
            margin-bottom: 0.3rem;
        }
        
        .full-message {
            color: #cbd5e0;
            font-size: 0.9rem;
            background: #1a1a1a;
            padding: 0.5rem;
            border-radius: 4px;
            border-left: 3px solid #4299e1;
            white-space: pre-wrap;
            word-break: break-word;
            margin-top: 0.3rem;
        }
        
        .additional-fields {
            margin-top: 0.5rem;
            padding-top: 0.5rem;
            border-top: 1px solid #4a5568;
        }
        
        .field {
            display: inline-block;
            background: #4a5568;
            color: #e2e8f0;
            padding: 0.2rem 0.5rem;
            margin: 0.1rem 0.3rem 0.1rem 0;
            border-radius: 4px;
            font-size: 0.8rem;
            font-family: 'Courier New', monospace;
        }
        
        .empty-state {
            text-align: center;
            padding: 4rem 2rem;
            color: #a0aec0;
        }
        
        .empty-state h3 {
            font-size: 1.2rem;
            margin-bottom: 0.5rem;
            color: #cbd5e0;
        }
        
        ::-webkit-scrollbar {
            width: 8px;
        }
        
        ::-webkit-scrollbar-track {
            background: #2d3748;
        }
        
        ::-webkit-scrollbar-thumb {
            background: #4a5568;
            border-radius: 4px;
        }
        
        ::-webkit-scrollbar-thumb:hover {
            background: #63b3ed;
        }
        
        @media (max-width: 768px) {
            .controls {
                padding: 1rem;
            }
            
            .log-entry {
                padding: 0.75rem;
            }
            
            .log-header {
                flex-direction: column;
                align-items: flex-start;
            }
        }
    </style>
</head>
<body>
    <div class="header">
        <h1>🔍 GELF Log Viewer</h1>
        <div class="stats">
            <span id="messageCount">0</span> messages • 
            <span id="capacity">0</span>% capacity • 
            Real-time streaming
        </div>
    </div>
    
    <div class="controls">
        <button class="btn" onclick="toggleStream()">
            <span id="streamBtn">Pause Stream</span>
        </button>
        <button class="btn danger" onclick="clearLogs()">Clear Display</button>
        <button class="btn" onclick="loadHistoryLogs()">Load History</button>
        <div class="status" id="status">
            <span id="statusText">Connecting...</span>
        </div>
    </div>
    
    <div class="main-content">
        <div class="log-container" id="logContainer">
            <div class="empty-state">
                <h3>Waiting for log messages...</h3>
                <p>GELF messages will appear here in real-time</p>
            </div>
        </div>
    </div>

    <script>
        let eventSource = null;
        let isStreaming = false;
        let logs = [];
        
        function formatTimestamp(timestamp) {
            return new Date(timestamp * 1000).toLocaleString();
        }
        
        function getLevelClass(level) {
            return level !== undefined ? `level-${level}` : 'level-6';
        }
        
        function getLevelText(level) {
            const levels = {
                0: 'EMERG', 1: 'ALERT', 2: 'CRIT', 3: 'ERR',
                4: 'WARN', 5: 'NOTICE', 6: 'INFO', 7: 'DEBUG'
            };
            return levels[level] || 'INFO';
        }
        
        function createLogEntry(log) {
            const entry = document.createElement('div');
            entry.className = 'log-entry';
            
            const additionalFields = Object.entries(log)
                .filter(([key, value]) => key.startsWith('_') && value !== null && value !== undefined)
                .map(([key, value]) => `<span class="field">${key}: ${value}</span>`)
                .join('');
            
            entry.innerHTML = `
                <div class="log-header">
                    <div>
                        <span class="log-level ${getLevelClass(log.level)}">${getLevelText(log.level)}</span>
                        <span class="host">${log.host || 'unknown'}</span>
                    </div>
                    <span class="timestamp">${formatTimestamp(log.received_at)}</span>
                </div>
                <div class="message">
                    <div class="short-message">${log.short_message || 'No message'}</div>
                    ${log.full_message ? `<div class="full-message">${log.full_message}</div>` : ''}
                </div>
                ${additionalFields ? `<div class="additional-fields">${additionalFields}</div>` : ''}
            `;
            
            return entry;
        }
        
        function addLogEntry(log) {
            const container = document.getElementById('logContainer');
            const emptyState = container.querySelector('.empty-state');
            
            if (emptyState) {
                emptyState.remove();
            }
            
            const entry = createLogEntry(log);
            container.insertBefore(entry, container.firstChild);
            
            // Keep only last 1000 entries for performance
            while (container.children.length > 1000) {
                container.removeChild(container.lastChild);
            }
        }
        
        function updateStats() {
            fetch('/stats')
                .then(response => response.json())
                .then(data => {
                    document.getElementById('messageCount').textContent = data.total_messages;
                    document.getElementById('capacity').textContent = data.capacity_used_percent.toFixed(1);
                })
                .catch(console.error);
        }
        
        function startStream() {
            if (eventSource) {
                eventSource.close();
            }
            
            eventSource = new EventSource('/stream');
            
            eventSource.onopen = function() {
                console.log('SSE connection opened');
                document.getElementById('status').className = 'status connected';
                document.getElementById('statusText').textContent = 'Connected';
                isStreaming = true;
                document.getElementById('streamBtn').textContent = 'Pause Stream';
            };
            
            eventSource.onmessage = function(event) {
                const log = JSON.parse(event.data);
                addLogEntry(log);
            };
            
            eventSource.onerror = function() {
                console.log('SSE connection error');
                document.getElementById('status').className = 'status disconnected';
                document.getElementById('statusText').textContent = 'Disconnected';
                
                // Attempt to reconnect after 5 seconds
                setTimeout(() => {
                    if (isStreaming) {
                        console.log('Attempting to reconnect...');
                        startStream();
                    }
                }, 5000);
            };
        }
        
        function stopStream() {
            if (eventSource) {
                eventSource.close();
                eventSource = null;
            }
            isStreaming = false;
            document.getElementById('status').className = 'status disconnected';
            document.getElementById('statusText').textContent = 'Paused';
            document.getElementById('streamBtn').textContent = 'Resume Stream';
        }
        
        function toggleStream() {
            if (isStreaming) {
                stopStream();
            } else {
                startStream();
            }
        }
        
        function clearLogs() {
            const container = document.getElementById('logContainer');
            container.innerHTML = '<div class="empty-state"><h3>Display cleared</h3><p>New messages will appear here</p></div>';
        }
        
        function loadHistoryLogs() {
            fetch('/logs?limit=50')
                .then(response => response.json())
                .then(data => {
                    clearLogs();
                    data.reverse().forEach(log => addLogEntry(log));
                })
                .catch(console.error);
        }
        
        // Initialize
        document.addEventListener('DOMContentLoaded', function() {
            startStream();
            updateStats();
            setInterval(updateStats, 10000); // Update stats every 10 seconds
            
            // Load initial history
            loadHistoryLogs();
        });
        
        // Clean up on page unload
        window.addEventListener('beforeunload', function() {
            if (eventSource) {
                eventSource.close();
            }
        });
    </script>
</body>
</html>"#.to_string()
}