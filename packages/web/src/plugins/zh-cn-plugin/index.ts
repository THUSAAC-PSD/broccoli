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
      'problem.notFound': '未找到题目。',
      'problem.loadError': '加载题目失败。',

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

      // Contests
      'contests.title': '比赛',
      'contests.titleColumn': '比赛',
      'contests.status': '状态',
      'contests.startTime': '开始',
      'contests.endTime': '结束',
      'contests.searchPlaceholder': '搜索比赛...',
      'contests.empty': '暂无比赛。',
      'contests.upcoming': '即将开始',
      'contests.running': '进行中',
      'contests.ended': '已结束',
      'contests.description': '描述',
      'contests.noDescription': '暂无描述。',
      'contests.problems': '题目',
      'contests.notFound': '未找到比赛。',
      'contests.loadError': '加载比赛详情失败。',
      'contests.loadProblemsError': '加载比赛题目失败。',
      'contests.inDays': '{{count}} 天后',
      'contests.inHours': '{{count}} 小时后',
      'contests.inMinutes': '{{count}} 分钟后',
      'contests.daysAgo': '{{count}} 天前',
      'contests.hoursAgo': '{{count}} 小时前',
      'contests.minutesAgo': '{{count}} 分钟前',

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

      // Sidebar (admin)
      'sidebar.admin': '管理',

      // Admin
      'admin.title': '管理后台',
      'admin.contests': '比赛',
      'admin.problems': '题目',
      'admin.subtitle': '管理比赛、题目和平台设置。',
      'admin.createContest': '创建比赛',
      'admin.createContestDesc': '设置新的编程比赛，自定义规则和时间。',
      'admin.createProblem': '创建题目',
      'admin.createProblemDesc': '向题库中添加新题目。',
      'admin.createContestSuccess': '比赛创建成功。',
      'admin.createProblemSuccess': '题目创建成功。',
      'admin.createError': '创建失败，请检查输入。',
      'admin.creating': '创建中...',
      'admin.unauthorized': '您没有权限访问此页面。',
      'admin.noContests': '暂无比赛，请创建一个。',
      'admin.noProblems': '暂无题目，请创建一个。',
      'admin.new': '新建',
      'admin.edit': '编辑',
      'admin.delete': '删除',
      'admin.deleteConfirm': '确定要删除此项吗？此操作无法撤销。',
      'admin.deleteSuccess': '删除成功。',
      'admin.editContest': '编辑比赛',
      'admin.editProblem': '编辑题目',
      'admin.editSuccess': '更新成功。',
      'admin.editError': '更新失败，请检查输入。',
      'admin.actions': '操作',
      'admin.saving': '保存中...',
      'admin.contestProblems': '比赛题目',
      'admin.contestProblemsDesc': '管理此比赛关联的题目。',
      'admin.addProblem': '添加题目',
      'admin.field.problemId': '题目 ID',
      'admin.field.label': '标签',
      'admin.noContestProblems': '此比赛暂无题目。',
      'admin.addProblemError': '添加题目失败，请检查 ID 和标签。',
      'admin.adding': '添加中...',
      'admin.availableProblems': '可用题目',
      'admin.field.title': '标题',
      'admin.field.description': '描述',
      'admin.field.content': '内容（Markdown）',
      'admin.field.startTime': '开始时间',
      'admin.field.endTime': '结束时间',
      'admin.field.isPublic': '公开',
      'admin.field.submissionsVisible': '提交可见',
      'admin.field.showCompileOutput': '显示编译输出',
      'admin.field.showParticipantsList': '显示参赛者列表',
      'admin.field.timeLimit': '时间限制（ms）',
      'admin.field.memoryLimit': '内存限制（KB）',
      'admin.field.showTestDetails': '显示测试详情',
      'admin.field.options': '选项',
      'admin.field.createdAt': '创建时间',

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
