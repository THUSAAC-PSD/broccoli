import type { ComponentBundle, PluginManifest } from '@broccoli/sdk';

export const manifest: PluginManifest = {
  name: 'zh-cn-language',
  version: '1.0.0',
  description: 'Chinese (Simplified) language pack',
  author: 'Broccoli Team',
  enabled: true,
  translations: {
    'zh-CN': {
      // App
      'app.name': 'Broccoli OJ',
      'app.tagline': '在线评测系统',

      // Sidebar
      'sidebar.platform': '平台',
      'sidebar.account': '账户',
      'sidebar.dashboard': '仪表盘',
      'sidebar.problems': '题目',
      'sidebar.contests': '比赛',
      'sidebar.tutorials': '教程',
      'sidebar.profile': '个人资料',
      'sidebar.settings': '设置',

      // Navbar
      'nav.contestInfo': '比赛信息',
      'nav.problems': '题目',
      'nav.submissions': '提交记录',
      'nav.ranking': '排名',
      'nav.signIn': '登录',
      'nav.signUp': '注册',
      'nav.toggleMenu': '切换导航菜单',

      // Problem description
      'problem.title': '题目',
      'problem.description': '描述',
      'problem.input': '输入',
      'problem.output': '输出',
      'problem.examples': '样例',
      'problem.explanation': '解释',
      'problem.notes': '注意',
      'problem.toggleFullscreen': '切换全屏',

      // Code editor
      'editor.title': '代码',
      'editor.run': '运行',
      'editor.submit': '提交',
      'editor.toggleFullscreen': '切换全屏',

      // Submission result
      'result.title': '结果',
      'result.submitPrompt': '提交代码以查看结果',
      'result.judging': '评测中...',
      'result.time': '时间：{{value}}ms',
      'result.memory': '内存：{{value}}MB',
      'result.testCase': '测试点 #{{id}}',
      'result.noResults': '暂无测试结果',
      'result.accepted': '通过',
      'result.wrongAnswer': '答案错误',
      'result.timeLimit': '超时',
      'result.runtimeError': '运行错误',
      'result.pending': '等待中',

      // Problems
      'problems.title': '题目列表',
      'problems.id': '#',
      'problems.titleColumn': '标题',
      'problems.label': '编号',
      'problems.contest': '比赛',
      'problems.due': '截止时间',
      'problems.dueInDays': '{{count}} 天后',
      'problems.dueInHours': '{{count}} 小时后',
      'problems.dueInMinutes': '{{count}} 分钟后',
      'problems.dueEnded': '已结束',
      'problems.searchPlaceholder': '搜索题目...',
      'problems.empty': '暂无题目。',
      'problems.contestProblems': '比赛题目',

      // Ranking
      'ranking.title': '排名',
      'ranking.user': '用户',
      'ranking.solved': '解题数',
      'ranking.score': '分数',
      'ranking.penalty': '罚时',
      'ranking.searchPlaceholder': '搜索用户...',
      'ranking.empty': '暂无参赛者。',

      // Theme
      'theme.switch': '切换主题',
      'theme.dark': '暗色模式',
      'theme.light': '亮色模式',

      // Auth
      'auth.loginTitle': '登录',
      'auth.registerTitle': '创建账户',
      'auth.username': '用户名',
      'auth.password': '密码',
      'auth.confirmPassword': '确认密码',
      'auth.login': '登录',
      'auth.register': '注册',
      'auth.noAccount': '没有账户？',
      'auth.haveAccount': '已有账户？',
      'auth.logout': '退出登录',
      'auth.invalidCredentials': '用户名或密码错误。',
      'auth.usernameTaken': '用户名已被使用。',
      'auth.passwordMismatch': '两次输入的密码不一致。',
      'auth.validationError': '请检查输入后重试。',

      // Sidebar (additional)
      'sidebar.guest': '游客',

      // Language switcher
      'locale.switch': '语言',

      // Plugin: Amazing Button
      'plugin.amazingButton.label': '神奇按钮',
      'plugin.amazingButton.alert': '太神奇了！',
      'plugin.amazingButton.pageTitle': '神奇页面！',

      // Plugin: Notifications
      'plugin.notification.button': '通知',
    },
  },
};

export const components: ComponentBundle = {};
